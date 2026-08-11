# Basic MIDI loop data view plan

## Goals and scope

Add a read-only, piano-roll-style MIDI note view to the existing **details** bottom pane for a single selected primitive loop that owns MIDI channels. The view must work in native, engine-backed/offline, and browser/Web Audio builds, coexist with existing audio waveforms on mixed audio/MIDI loops, and use the same selected-loop details publication path.

In scope:

- Fetching selected-loop MIDI channel content through a targeted backend API.
- Bounded browser protocol transfer and stale-response handling for MIDI details.
- Publishing immutable MIDI detail state through `shoop_app_api`.
- Rendering note spans, loop region, and playback position in `shoop_egui` with basic zoom/pan behavior consistent with waveforms.
- Empty/loading states, tests, and user documentation.

Out of scope:

- Editing, quantizing, selecting, auditioning, or otherwise mutating MIDI events.
- Dedicated rendering of CC, pitch-bend, pressure, program-change, SysEx, or other non-note messages. They remain preserved in loop content but are not visualized by this basic view.
- Changes to recording, playback, import/export, session formats, or MIDI routing.

## Immutable acceptance criteria

1. When exactly one primitive loop with a MIDI channel is selected and the details pane is open, each MIDI channel is represented by a labeled, read-only note lane; note pitch is vertical and event time/duration is horizontal.
2. MIDI-only loops show the MIDI view without an erroneous “no audio waveform data” terminal state. Mixed loops show both their existing audio waveforms and MIDI lanes in stable channel order.
3. Note-on/note-off pairs are interpreted per MIDI channel and note number, including velocity-zero note-on as note-off. Malformed, unsupported, or unmatched messages do not panic or hide valid notes.
4. The MIDI lane indicates the loop region and current playback position and supports basic horizontal zoom and drag-to-pan without changing loop data.
5. Empty MIDI channels and independently loading audio/MIDI content have truthful, nonblocking UI states. Changing selection or loop content cannot display data from a previously selected/revised loop.
6. Native, fake/engine-backed, and browser backends provide equivalent selected-loop MIDI detail data. Browser transfer is bounded/chunked and does not capture or serialize the entire session merely to populate the details pane.
7. Existing details-pane selection behavior, audio waveform rendering, MIDI recording/playback, and session persistence remain unchanged.

## Design rules and constraints

- Keep backend MIDI content authoritative. The application and UI receive snapshots only; opening or interacting with details must issue no content mutation.
- Extend the existing `Backend` selected-content boundary rather than reaching from `shoop_app` into engine/native internals or using `capture_session()` for UI refreshes.
- Keep browser commands/events within protocol capacities. Assemble one bounded chunk at a time, identify requests by loop and generation/revision, reject malformed bounds, and discard stale/out-of-order results after selection, content, or worklet lifecycle changes.
- Store fetched/derived detail data behind shared immutable allocations so the application’s frequently published snapshots do not deep-copy or repeatedly parse a large MIDI sequence.
- Pair notes deterministically by `(MIDI channel, note number)` in event order. Define and test handling of overlapping starts and unmatched ends/starts; clamp drawing to finite widget bounds and the channel timeline.
- Keep audio and MIDI loading independent so one media type can render while the other is pending or absent.
- Reuse details-pane visual conventions and control-safe scroll behavior. Keep MIDI-specific rendering isolated in a focused widget/module rather than expanding `details_pane.rs` with parsing and painting logic.
- Preserve target portability: shared API/state must not depend on native-only MIDI crates, and browser code must remain valid for `wasm32-unknown-unknown`.

## Staged implementation

### Stage 1 — Targeted backend MIDI detail contract

- [x] Add backend DTOs for MIDI channel detail metadata/events and a targeted `Backend` read API, including enough identity/revision information to detect content changes.
- [x] Implement immediate reads for `FakeBackend`, `EngineBackend`, and the native backend using their existing loop-channel/content snapshot ownership; include MIDI-only, empty, start-offset, and multi-channel cases.
- [x] Add backend contract tests proving event bytes/order, channel count/metadata, absent-channel behavior, revision consistency, and non-mutation.
- [x] Verify with targeted `shoop_backend` and native backend tests.
- [x] Commit the backend contract and implementations.

**Dependency:** This contract is required by browser transport and application publication.

### Stage 2 — Bounded browser/worklet retrieval

- [x] Extend `shoop_audio_protocol` with bounded request/response chunk types for one loop’s MIDI channels, carrying request generation, content revision, channel/event offsets, metadata, and completion state.
- [x] Handle requests in `shoop_audio_worklet` through the Stage 1 backend API, enforce protocol limits, and return explicit errors for invalid/stale requests.
- [x] Add browser adapter assembly state analogous to waveform assembly: request successive chunks, restart on content revision changes, expose `None` while incomplete, and clear assemblies on content mutation, session/worklet replacement, or loop deselection lifecycle.
- [x] Test protocol round trips and limits, worklet extraction, complete multi-chunk assembly, empty channels, stale/out-of-order responses, malformed chunks, and recovery after restart.
- [x] Verify targeted `shoop_audio_protocol`, `shoop_audio_worklet`, and `shoopdaloop` browser-audio tests plus Wasm compilation.
- [x] Commit the browser protocol and adapter milestone.

**Dependency:** Requires Stage 1; Stage 3 relies on the same `Some(complete) / None(pending)` semantics on every target.

### Stage 3 — Application detail state and selected-media lifecycle

- [x] Extend `shoop_app_api::LoopDetailsState` with MIDI channel presentation state and an independent MIDI loading flag, while retaining existing audio state compatibility.
- [x] Cache selected-loop MIDI content in `LoopModel`, fetch it alongside selected audio through a renamed/generalized selected-media refresh path, and clear both caches on deselection, clear/record/replace/import/session replacement, or other existing content-invalidating operations.
- [x] Convert raw backend events once per content revision into immutable UI-facing event/note data, preserving channel labels/roles, frame positions, loop length/start offset, and playback position.
- [x] Update application tests for MIDI-only, mixed, empty, and multiple-selection states; asynchronous loading; content invalidation/refetch; note edge cases; and unchanged audio waveform publication.
- [x] Verify targeted `shoop_app` tests, including cooperative engine-backed selection/recording workflows.
- [x] Commit the application-state milestone.

**Dependency:** Requires Stages 1–2 so application behavior is target-independent.

### Stage 4 — egui MIDI lane and details-pane composition

- [x] Add a dedicated MIDI sequence widget that caches derived note geometry, paints a bounded piano-roll lane with pitch/time mapping, loop-region shading, and playback cursor, and implements horizontal zoom/pan without emitting actions.
- [x] Compose MIDI lanes and audio waveform widgets in the details pane’s existing vertical scroll area; reset per-loop widget state on selection changes and present independent loading, no-audio, empty-MIDI, and no-media messages without suppressing available content.
- [x] Add focused tests for note pairing (channels, velocity-zero offs, overlap/unmatched/malformed events), coordinate/visibility calculations, cache identity, narrow/large layouts, and paint output for MIDI-only and mixed detail fixtures.
- [x] Verify targeted `shoop_egui` tests and existing details/piano pane interaction tests.
- [x] Commit the UI milestone.

**Dependency:** Requires Stage 3’s immutable presentation state.

### Stage 5 — Documentation and end-to-end validation

- [x] Update `docs/source/usage.loopcontrols.rst` and `src/rust/shoopdaloop/README.md` to describe the read-only MIDI details lane and its basic interaction/non-note limitation.
- [x] Exercise recorded MIDI-only and mixed audio/MIDI detail fixtures through deterministic native application/UI tests; verify selection changes, empty data, playback cursor, zoom/pan, and that details interaction never alters exported/session MIDI.
- [x] Exercise the same details flow in Chromium/Web Audio, including data larger than one protocol chunk and worklet/session restart, and confirm stale notes never remain visible.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [x] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`.
- [x] Run `python3 scripts/check_tracing_coverage.py --require-closed` and the relevant Wasm builds/browser smoke checks documented in `src/rust/shoopdaloop/README.md`.
- [x] Commit documentation and final validation fixes as the final milestone.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
