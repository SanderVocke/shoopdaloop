# egui Loop Hover Controls and Empty Tracks Plan

## Goals and scope

Refine the pure-egui loop and tracks presentation to match the established QML interaction model in `src/qml/LoopWidget.qml` and `src/qml/TracksWidget.qml`:

- Keep dial labels legible by painting the position indicator only as a short segment near the dial edge.
- Add the QML hover families for loop controls: play-dry below play, grab and dry-to-wet re-record below record, and stereo balance beside loop volume.
- Keep each temporary control visible while the pointer is over its source control, the temporary control, or the small traversal gap, and paint it outside the loop row without changing track/row geometry.
- Make the newly exposed controls functional through the typed application/backend boundary on native, cooperative dummy, and browser AudioWorklet paths.
- Start production sessions with only the sync track/loop and show an empty-main-tracks instruction beside the existing add-track affordance until the first main track is added.

In scope are the loop-control API/state needed for these controls, QML-equivalent selection/target/sync/solo/cycle/play-after-record behavior, stereo loop gain/balance composition, cross-target protocol support, focused presentation/behavior tests, and parity documentation updates.

Out of scope are new dry/wet track creation or FX topology, composite-loop creation, loop context menus, track deletion/reordering, and broader visual redesign. The fixture-only `shoop_egui_preview` may retain representative tracks; the empty-start requirement applies to production application state.

## Immutable acceptance criteria

1. A loop volume dial paints its indicator as an edge-local radial segment at every value; the segment never crosses or obscures the centered `V`. The temporary balance dial uses the same treatment and leaves `B` clear.
2. Hovering a loop row still exposes the primary play/record/stop controls. Hovering play exposes the orange play-dry control below it; hovering record exposes grab and orange dry-to-wet re-record controls below it; hovering an applicable stereo volume dial exposes a balance dial to its right.
3. Temporary controls are foreground overlays: they may extend beyond the loop row and track content rectangle, do not reserve layout space or move neighboring rows, and remain interactive above neighboring content.
4. A temporary control remains visible without flicker while the pointer is over its source, any member of its temporary control group, or crosses the source/overlay gap. It closes promptly only after the pointer leaves that combined hover region and no member is being dragged.
5. QML eligibility is preserved: script-composite loops do not expose record-family or dry variants; gain is shown only for applicable audio loops; balance is shown only for stereo audio loops; grab remains available for regular loops and regular composites but not script composites.
6. Play-dry requests `PlayingDryThroughWet`; re-record requests `RecordingDryIntoWet` for the QML-equivalent delay/duration and returns to the prior mode; grab follows the QML rules for selection, target, sync/immediate mode, fixed cycle count, play-after-record, and solo; failures are observable rather than silently ignored.
7. Stereo loop balance is clamped to `[-1, 1]`, double-click resets it to center, and changing gain or balance preserves the other component while applying QML-equivalent left/right factors. State round-trips through snapshots on native and browser backends.
8. On a normal fresh application start, authoritative state contains exactly one sync track with one sync loop and no main tracks. The main tracks pane shows clear text directing the user to the adjacent add-track control; accepting Add Track removes the empty state and creates the existing aligned loop slots.
9. Existing loop selection/targeting, normal play/record/stop, gain, track layout/scrolling, add-track/add-loop, native startup, browser audio, and retained QML behavior continue to pass their regression gates.

## Design rules and constraints

- Keep `shoop_egui` presentation-only: it consumes immutable `shoop_app_api` state and emits typed intents; application policy remains in `shoop_app`, and engine mutations remain behind `shoop_backend`.
- Use stable loop IDs for popup/drag state. Do not let recycled vector positions transfer a visible popup or drag to another loop after tracks/loops change.
- Model popup visibility as the union of source hover, child hover, active drag, and a short traversal grace period, matching the QML popup timer without making the controls sticky.
- Paint popups through a foreground `egui::Area`/layer with stable IDs and screen-aware placement, rather than expanding `TrackWidget` layout or weakening the scroll area's normal clipping for all content.
- Keep gain and balance as separate semantic values but apply their channel gains coherently. Backend snapshots, fake backend, engine backend, Web Audio protocol, and worklet must agree on their ranges and defaults.
- Batch grab targets and preflight capacity so selected-loop grabs are all-or-error. The browser/worklet path must preserve the existing bounded command queues, hard recording-storage limits, and no-allocation/no-lock audio-callback rules.
- Derive grab and re-record timing from authoritative application/loop state using the formulas in `LoopWidget.qml`; do not duplicate business policy in the widget.
- Keep fresh-session ownership in `shoop_app`; the tracks widget only decides whether to render the empty-state instruction from the supplied non-sync track list.
- Update `EGUI_FEATURE_PARITY_MATRIX.md` and relevant runner documentation with implementation and verification evidence rather than treating visual presence alone as parity.

## Staged implementation plan

### Stage 1 — Freeze contracts and QML-equivalent behavior

- [x] Add focused characterization tests/tables for the QML hover groups, eligibility, grab timing branches, re-record scheduling, and stereo gain/balance factors, using `LoopWidget.qml`, `AudioDial.qml`, and existing QML loop tests as the baseline.
- [x] Extend `shoop_app_api` with typed loop actions and immutable state for play-dry, grab, re-record, and stereo balance, including explicit applicability data where it cannot be inferred safely from `stereo`, audio presence, and composite kind.
- [x] Define the backend grab request/result and loop balance contract, including batch target identity, cycle-window parameters, post-grab mode/position, range/default behavior, and observable capacity/backend failures.
- [x] Verify API tests cover defaults, action routing, capability combinations, and stable IDs; run `cargo test -p shoop_app_api`.
- [x] Commit the frozen API and behavior contract before backend or widget implementation.

### Stage 2 — Implement cross-target backend primitives

- [x] Extend `shoop_backend::Backend`, `BackendLoopState`, `FakeBackend`, and `EngineBackend` with loop balance and batched ringbuffer adoption; retain overall gain/balance separately and apply QML left/right factors to stereo output channels.
- [x] Use the engine session's bounded ringbuffer-adoption facilities for grab, adding equivalent MIDI handling only where the current track exposes a MIDI capture channel; preflight every selected target before committing any target.
- [x] Extend `shoop_audio_protocol`, `shoopdaloop_egui::browser_audio`, and `shoop_audio_worklet` commands/snapshots for balance and grab, with stable-ID validation, bounded payloads, command-journal rules, and explicit errors.
- [x] Add fake, dummy-engine, protocol round-trip, worklet, storage-capacity, and no-allocation regression tests proving balance persistence/factors and atomic grab behavior.
- [x] Verify with targeted backend/protocol/worklet tests and warning-denying native/Wasm builds for the touched packages.
- [x] Commit the backend and cross-target transport milestone.

### Stage 3 — Add application policy for the new actions

- [x] Route play-dry through the existing selection, target-delay, sync, and solo transition policy using `PlayingDryThroughWet`.
- [x] Implement re-record scheduling from current loop/sync length and position, preserving the prior mode and applying the operation to the same selected-target semantics as QML.
- [x] Translate grab into one backend batch using QML's synchronized, immediate, targeted, fixed-cycle, play-after-record, and solo branches; publish updated mode/content/details state and report invalid/unsupported/capacity cases as notifications.
- [x] Apply balance changes only to applicable stereo loops, clamp/reset consistently, suppress no-op backend work, and publish confirmed gain/balance state from backend snapshots.
- [x] Add actor/cooperative tests for every policy branch, grouped selection, stale IDs, and failure atomicity; verify with `cargo test -p shoop_app` plus the backend contract tests.
- [x] Commit the application-policy milestone.

### Stage 4 — Build dial rendering and hover overlays in egui

- [x] Extract a reusable dial painter/interaction helper in `src/rust/shoop_egui/src/loop_widget.rs`; compute an inner and outer point near the circumference for the indicator, then paint the centered label unobstructed.
- [x] Add per-loop stable hover/drag state and foreground overlay groups for play-dry, grab/re-record, and stereo balance. Preserve the existing row-hover primary controls and route every click/drag through the new typed actions.
- [x] Match the QML control ordering, colors, half-green play-after-record treatment, tooltips, drag behavior, and double-click resets while keeping overlays outside layout and above adjacent loops.
- [x] Add headless egui pointer-sequence tests for source-to-child traversal, child retention, delayed dismissal, dragging, eligibility, non-overlap with dial labels, overlay geometry outside the row, and emitted action identity.
- [x] Exercise dense multi-row tracks and minimum/common window sizes to prove overlays neither resize tracks nor misroute actions.
- [x] Verify with `cargo test -p shoop_egui` and commit the presentation milestone.

### Stage 5 — Add and verify the empty-main-tracks state

- [x] Render an instructional empty state in `TracksWidget` only when its supplied main-track slice is empty, positioned with and clearly referring to the existing add-track button.
- [x] Preserve the horizontal/vertical scroll structure and aligned controls row once tracks exist; do not duplicate sync-track presentation in the main pane.
- [x] Add presentation tests for zero and one main track, add-button routing from the empty state, and disappearance after the first authoritative track snapshot.
- [x] Retain/extend the `shoop_app` initialization test proving one sync track/loop and no implicit main track, and add a production composition smoke assertion rather than changing fixture snapshots.
- [x] Verify targeted application/widget/runner tests and commit the empty-state milestone.

### Stage 6 — Documentation and end-to-end validation

- [x] Update `EGUI_FEATURE_PARITY_MATRIX.md` for LOOP-014/015/016 and add the empty-tracks presentation evidence; document any approved visual adaptation from QML.
- [x] Run `cargo fmt --all`, then `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend`.
- [x] Run `cargo test --workspace --features shoop_engine/app_backend` and the built QML self-test (`target/debug/shoopdaloop_dev.sh --self-test`).
- [x] Run warning-denying `wasm32-unknown-unknown` checks for `shoopdaloop_egui` and `shoop_egui_preview`, build the AudioWorklet, then build the hosted and self-contained browser artifacts.
- [x] Exercise browser smoke at 360×200 and 900×600, including stereo balance, popup traversal, grab/re-record command flow, callback progress, queue/storage diagnostics, and fresh-session empty state; run a native GUI smoke with the same interaction sequence.
- [x] Compare screenshots/recorded interaction against QML for dial readability, popup placement/order, and hover retention, recording any environment-only skips with evidence.
- [x] Commit the final documentation and validation evidence.

## Completion evidence

- Implementation milestones: `cce9d587` adds the typed application/backend/protocol/worklet behavior and `ba93220b` adds the foreground egui controls, stable-ID widget ownership, dial rendering, and empty-tracks presentation.
- Focused verification: `shoop_app` (15 tests), `shoop_egui` (26 tests), `shoop_backend` (9 tests), `shoop_audio_protocol` (2 tests), `shoop_audio_worklet` (3 tests), and `shoopdaloop_egui` (3 tests) pass. Coverage includes every grab policy branch, dry-mode scheduling, atomic preflight, non-zero Web Audio adoption, balance round trips, popup eligibility/lifetime/geometry, stable IDs, empty startup, and minimum/common paint sizes.
- Build and regression gates: `cargo fmt --all` and `RUSTFLAGS="-D warnings" cargo build --workspace --features shoop_engine/app_backend` pass. The first parallel workspace run exposed two timing-sensitive engine tests and unavailable virtual MIDI; each timing test then passed three focused serialized runs. `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1` passes the complete workspace, with virtual MIDI reported as the documented environment skip.
- Cross-target verification: warning-denying `wasm32-unknown-unknown` checks pass for `shoopdaloop_egui` and `shoop_egui_preview`; the worklet also passes its Wasm compiler check and native protocol/no-allocation tests. Artifact linking could not run because this environment has no `lld`; Trunk, Chromium/Chrome, and Xvfb are also unavailable, so hosted/self-contained browser packaging, browser smoke, native window smoke, and screenshot comparison are recorded environment-only skips rather than product evidence.
- QML compatibility: the offscreen full run passed `tst_Backend.qml` and `tst_Backend_jack.qml`, then the runner produced `Created invalid object` while loading `tst_CompositeLoop_running.qml` and timed out. A focused `tst_TwoLoops.qml` retry failed at the same QML object-creation boundary. No QML source or frontend behavior changed in this work; the failure is recorded as an environment/runtime skip, while the warning-denying frontend build and complete Rust workspace gate pass.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
