# Dry/wet recording alignment plan

## Goal

Make an ordinary recording on a dry/wet track publish correctly aligned per-channel media: direct and dry channels use the effective recording offset, while wet channels use that offset plus the effective processor latency. Keep processor render advance exact-once during dry-through-wet operations, add independent Automatic/Manual/Automatic + trim controls for processor latency, and allow explicit correction of both the common take alignment and the wet-relative-to-dry alignment after recording.

Carla is deliberately not inspected for plugin or graph latency. Its automatic processor value is zero; users compensate Carla with Manual or Automatic + trim.

## Scope

- Resolve and persist recording alignment and processor latency as independent track settings.
- Derive channel-role-specific alignment and retention for ordinary simultaneous dry/wet recording.
- Preserve existing direct recording, normal wet playback/export, dry-through-wet playback, and dry-into-wet rendering semantics.
- Add a narrow completed-take correction for the wet-relative-to-dry processor differential.
- Keep Fake, Engine, Native, browser/Worklet, application, session, and UI behavior consistent.
- Update the session and browser protocol formats with explicit compatibility handling.

Out of scope:

- Querying, parsing, measuring, or inferring Carla plugin/graph latency.
- Reintroducing generalized provider identities, histories, confidence, provenance, diagnostics, or policy selection.
- Per-plugin, per-route, or arbitrary per-channel latency policy UI.
- Expanding compensation support for retrospective grab or ordinary replacement beyond their current supported bounds.
- Treating creative delay effects as processor latency.

## Immutable acceptance criteria

1. For an ordinary recording with effective recording offset `R` and effective processor latency `P`, Direct and Dry channels latch capture alignment `R`, while Wet channels latch checked capture alignment `R + P`.
2. Recording preparation reserves the sign-derived retained window separately for every channel's derived alignment. Mixed-sign cases such as negative `R` and positive `R + P` retain both required preroll and postroll, and the take is published only after every channel settles.
3. If any derived alignment is out of bounds, any channel cannot prepare or complete its retained window, or checked arithmetic fails, the operation fails atomically and publishes no partially corrected take.
4. Ordinary recording changes annotations and retained windows only; it does not delay live monitoring or apply processor render advance to the incoming signal.
5. Normal playback and logical audio/MIDI export use each channel's stored capture alignment. Wet playback therefore consumes `R + P` without applying `P` again.
6. Play-dry-through-wet and record-dry-into-wet continue to apply processor render advance exactly once. A dry-into-wet destination remains canonical and does not receive the ordinary-record wet alignment a second time.
7. Recording alignment and processor latency each expose independent Automatic, Manual, and Automatic + trim adjustment modes. Recording values remain signed; effective processor latency remains non-negative and bounded; processor trim is signed and its resolved result is validated.
8. Carla processor automatic latency is exactly zero. No Carla API, state parsing, signal measurement, plugin enumeration, or graph inference is introduced. Manual and Automatic + trim provide Carla compensation.
9. Track settings are resolved and latched before the operation boundary. Changing settings later affects future operations only and does not move a completed take or alter an armed operation.
10. The existing completed-take alignment edit continues to apply one common delta to all retained channels. A separate processed-take correction changes the wet-relative-to-dry differential by applying one atomic delta to Wet channels only, preserving differences within the dry group and within the wet group.
11. Every completed-take correction preflights all affected retained audio/MIDI windows and either commits every channel or changes nothing. It is unavailable when the take has no meaningful dry/wet pairing.
12. Existing sessions migrate their stored processor advance to Manual mode without changing playback. New sessions persist processor adjustment mode and manual/trim input, never transient automatic observations; per-channel dry/wet annotations round-trip and resample correctly.
13. Fake, Engine, Native, browser/Worklet, application state, save/load, duplication/import, waveform/details coordinates, and normal exports agree on the resulting per-channel mappings.
14. No latency resolution, checked derivation, allocation, locking, graph rebuild, provider query, or unbounded work is added to the realtime callback.
15. The latency compensation dialog clearly separates Recording alignment, Processor latency, and completed-take corrections, and displays effective values plus actionable pending/error feedback without adding an advanced diagnostics panel.

## Design rules and constraints

- Keep one track-level base recording offset `R` and one track-level processor differential `P`; derive role-specific recording annotations rather than introducing independent channel policies.
- Use channel roles as the only mapping rule: Direct/Dry use `R`; Wet uses `R + P`. Apply the same rule to future Wet MIDI channels, although current dry/wet topologies retain only Dry MIDI.
- Precompute and validate the derived wet offset on the control/preparation path. Publish only compact prepared/latched integers to callback-owned state.
- `P` is one uniform processor differential. If a future processor cannot justify one common value for all wet channels, automatic inference must remain unavailable or zero according to the backend contract rather than inventing per-route estimates.
- Automatic processor state is a current optional/exact value only; do not add source metadata, revisions exposed to users, history, smoothing, or confidence.
- For Carla and other explicitly unsupported processor detectors in this scope, use an automatic baseline of zero. Do not inspect Carla internals.
- Preserve immutable completed-take behavior: track-setting changes never rewrite existing channel annotations. Only explicit take-correction intents may do so.
- Preserve current replacement/grab boundaries. Preflight must consider derived per-channel offsets so a base offset of zero cannot accidentally admit an unsupported replacement with nonzero wet alignment.
- Continue using checked frame arithmetic and the existing maximum compensation/retention bounds.
- Keep backend parity logic narrow and share pure resolution/derivation helpers where doing so prevents Fake, Engine, Native, and Worklet drift.
- Maintain callback allocation and lock guarantees with the existing realtime tests and warning-denied builds.

## Stage 1: Define and test the latency domain

Depends on: none.

- [x] Add a typed processor-latency adjustment model with Automatic, Manual override, and Automatic + signed trim resolution.
- [ ] Represent automatic processor latency, user input, and effective processor latency separately; keep the effective callback value as `ProcessorRenderAdvance`.
- [x] Add checked derivation for the ordinary-record wet alignment `R + P`, including non-negative processor resolution and combined alignment bounds.
- [x] Define compact prepared values that make both direct/dry and wet recording offsets available without callback-time arithmetic.
- [x] Add table-driven domain tests for positive, negative, mixed-sign, zero, maximum, underflow, overflow, unavailable, and invalid-negative processor cases.

Verification:

- [x] Run `cargo test -p shoop_latency`.
- [x] Run warning-denied build/Clippy for the touched latency crate.

## Stage 2: Apply role-specific retention and latching in the engine

Depends on: Stage 1.

- [x] Change `AudioMidiLoop::prepare_latency` to prepare each audio/MIDI channel from its role-derived ordinary-record alignment instead of one shared retention window.
- [x] Change ordinary `Recording` latching so Direct/Dry channels receive `R` and Wet channels receive `R + P`.
- [x] Keep `PlayingDryThroughWet` dry render advance and `RecordingDryIntoWet` canonical wet destination behavior separate from ordinary-record capture annotations.
- [x] Ensure per-channel postroll completion, exhaustion, abort, and publication remain atomic when dry and wet require different windows.
- [x] Update replacement preparation/latching guards so all derived channel alignments are considered and unsupported nonzero replacement still fails before mutation.
- [ ] Add deterministic audio and MIDI tests proving role mapping, mixed preroll/postroll, wrap behavior, exact-once render advance, canonical dry-into-wet output, and no callback allocations.

Verification:

- [x] Run focused `shoop_engine` latency/channel/loop tests.
- [ ] Run the complete `shoop_engine` test suite, including callback allocation and transition tests.

## Stage 3: Resolve processor settings and enforce backend parity

Depends on: Stages 1-2.

- [ ] Extend `BackendTrackLatencyState` with processor automatic value, adjustment mode, manual/trim input, effective value, pending state, and actionable errors.
- [ ] Update the shared backend resolver to validate `R`, `P`, and the derived wet mapping atomically before publishing prepared values.
- [ ] Apply identical semantics in Fake, Engine, delegated, and Native backends; avoid copy-specific arithmetic.
- [ ] Set Carla processor automatic latency to zero without adding any Carla host query or inference path. Define zero automatic baselines for other unsupported processor detectors where needed for backend parity.
- [ ] Preserve arming guards so processor-setting edits cannot replace a prepared operation's values.
- [ ] Update ordinary replacement/grab preflight, future-loop inheritance, track duplication, route restoration, and processor restoration to use the resolved processor configuration.
- [ ] Add backend tests for `R`/`R + P` channel annotations, mixed retained windows, zero-based Carla Automatic + trim, future-operation semantics, and atomic failures across all backend implementations.

Verification:

- [ ] Run complete `shoop_backend` tests.
- [ ] Run focused warning-denied backend Clippy.
- [ ] Confirm case-insensitive removed-architecture terminology searches remain clean.

## Stage 4: Add explicit completed-take processor correction

Depends on: Stage 3.

- [ ] Keep `SetTakeAlignment` as a common-delta edit across every retained channel.
- [ ] Add a typed take-processor-alignment intent/backend operation that derives the current dry-to-wet differential and applies one requested delta only to Wet channels.
- [ ] Define a stable reference when multiple dry/wet channels exist, preserve intra-group differences, and reject takes without a usable dry/wet pairing.
- [ ] Preflight the candidate alignment and retained logical window for every affected Wet channel before mutating any channel.
- [ ] Fence the edit while playback, recording, replacement, dry-into-wet rendering, postroll, or another relevant mutation is active.
- [ ] Invalidate/reload application and browser media/timeline caches after success or asynchronous rejection, matching existing alignment-edit recovery behavior.
- [ ] Add Fake, Engine, Native, browser, and application tests for success, preserved group differences, unavailable topology, bounds failure, atomicity, and stale-cache recovery.

Verification:

- [ ] Run focused backend, application, and Worklet mutation/recovery tests.
- [ ] Confirm existing common take-alignment tests still pass unchanged in intent.

## Stage 5: Update application API, browser protocol, and Worklet

Depends on: Stages 3-4.

- [ ] Extend application state and intents with processor adjustment, automatic/manual/trim values, effective processor latency, and processed-take differential correction.
- [ ] Update application-to-backend mapping and optimistic/pending/error handling without conflating recording and processor adjustment modes.
- [ ] Extend the audio protocol commands and snapshots, increment the protocol version, and update serialization/golden round trips.
- [ ] Update Worklet command handling, state publication, mutation detail typing, rejection recovery, and client cache invalidation.
- [ ] Preserve browser's zero automatic processor baseline and manual/trim behavior without introducing timing estimation.
- [ ] Add protocol compatibility, app dispatch, browser retry/rejection, and state round-trip tests.

Verification:

- [ ] Run `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_worklet_client`, and `shoop_app` tests.
- [ ] Run the Chromium-backed engine/backend/application/Worklet suites required by project guidance.

## Stage 6: Persist and resample the new configuration

Depends on: Stages 3 and 5.

- [ ] Introduce the next explicit session document version for processor adjustment mode and signed manual/trim input.
- [ ] Migrate version-7 `processor_advance_frames` to Manual override with the identical effective value.
- [ ] Do not serialize automatic processor observations or inferred provider metadata.
- [ ] Preserve already stored per-channel capture alignments, including differing dry/wet values, through save/load, archive validation, duplication, partial import, and replacement of loop content.
- [ ] Scale processor manual/trim values and every channel annotation with checked deterministic rounding during sample-rate conversion; revalidate retained windows after conversion.
- [ ] Add current-version round trips, version-7 migration, malformed/out-of-range input, mixed dry/wet alignment, and multi-rate resampling tests.

Verification:

- [ ] Run complete `shoop_session` archive/document/resampling tests.
- [ ] Run application save/load/export tests over migrated and current sessions.

## Stage 7: Update the latency compensation dialog and documentation

Depends on: Stages 4-6.

- [ ] Split the existing dialog into clear Recording alignment and Processor latency sections, each with its own Automatic/Manual/Automatic + trim selector, value editor, effective value, pending state, and actionable error text.
- [ ] For Carla, show an automatic processor value of zero and permit Manual or Automatic + trim without implying that Carla was inspected.
- [ ] For each completed processed take, expose common take alignment and wet-relative processor alignment corrections; hide or disable the latter for non-dry/wet takes.
- [ ] Keep all controls in the track-level Latency compensation dialog and out of the compact `⋮` menu itself.
- [ ] Update user documentation, session-format documentation, `LATENCY_REMAINING_WORK.md`, the simplification audit, and tracing coverage where behavior or instrumentation changes.
- [ ] Replace the ignored visual-validation artifact with a screenshot of the revised dialog at a supported common window size.
- [ ] Add UI interaction/layout tests covering direct, Carla, manual, trim, unavailable/invalid, pending/error, and completed processed-take states.

Verification:

- [ ] Run complete `shoop_egui` tests at minimum and common sizes.
- [ ] Inspect native and browser layouts and confirm the dialog remains usable with many completed takes.
- [ ] Run documentation and repository policy checks.

## Stage 8: End-to-end validation

Depends on: all previous stages.

- [ ] Record a deterministic dry/wet fixture with known `R` and `P`; prove raw dry and wet impulses retain their physical separation while both map to the same logical frame through annotations `R` and `R + P`.
- [ ] Prove normal wet playback and WAV/Shoop audio plus standard/exact MIDI export apply the stored mapping without applying processor latency twice.
- [ ] Prove play-dry-through-wet and record-dry-into-wet apply `P` exactly once across loop boundaries, non-divisible callback sizes, and planned transitions.
- [ ] Exercise positive, negative, zero, mixed-sign, maximum, insufficient-retention, storage-exhaustion, cancellation, and postroll re-entry cases.
- [ ] Verify common take correction and wet-relative correction across save/load, duplication/import, resampling, details/waveform reads, browser optimistic rejection, and cache reload.
- [ ] Verify Carla Automatic resolves to zero and Carla Manual/Automatic + trim drive both dry-through-wet advance and ordinary-record wet annotation without any Carla inference code.
- [ ] Run native workspace tests, Node/Chromium suites, warning-denied workspace build, focused warning-denied Clippy, formatting, diff checks, test-attribute policy, tracing coverage, and removed-symbol/terminology searches.
- [ ] Record final evidence and map every acceptance criterion to concrete tests, documentation, or artifacts before declaring completion.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
