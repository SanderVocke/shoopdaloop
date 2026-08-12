# Composite loop content view milestone

## Goal and scope

Add a read-only composite-loop content view to the egui details pane. It will replace the primitive-media empty-state for a single selected regular or script composite and visualize its schedule in the style of the removed QML `EditCompositeLoop.qml`: ordered horizontal track rows, named loop rectangles positioned by time, and extra vertical swimlanes when rectangles on one track overlap.

This milestone includes the application snapshot data needed for an accurate view, deterministic layout, horizontal/vertical scrolling, and horizontal zoom. It does not add composition mutation, drag/drop, resize/duplicate, mode/kind changes, link indicators, context menus, or other editor operations. Look-and-feel approval remains a manual user check after implementation and CI completion; implementation must not launch or drive the GUI interactively.

## Immutable acceptance criteria

1. With exactly one regular or script composite selected, opening **details** shows the selected loop title and a read-only composite timeline rather than “no audio or MIDI data.”
2. The timeline has one labeled row for each main track in session order. Every scheduled source loop is a rectangle in its source track row, horizontally positioned and sized to represent its scheduled timespan, with its current loop name visible when space permits.
3. Events from all playlists and parallel sections are represented. Serial sections, per-event delays, natural/forced durations, and regular versus script composition data are not lost in the snapshot/view path.
4. Overlapping timespans on the same track are assigned deterministic separate swimlanes; touching, non-overlapping timespans may reuse a lane. A track row grows vertically to fit exactly the lanes it needs.
5. The view offers a bounded horizontal zoom control and both horizontal and vertical scrolling when content exceeds the details pane. Zooming changes timeline scale without changing application/composition state.
6. Empty composites render a clear empty-schedule message. Primitive audio/MIDI details, selection behavior, native builds, and browser builds continue to work as before.
7. Layout/snapshot behavior is covered by non-interactive automated tests. The repository’s required formatting, warning-denying build, tracing, workspace tests, and relevant WebAssembly checks pass.
8. Before handoff for the user’s manual look-and-feel test, the work is committed by meaningful stage, pushed to a pull request against `master`, and the unchanged final PR head has three consecutive complete green required-CI runs. Any failure or new push resets the consecutive-green count.

## Design rules and constraints

- Use commit `a045e1f0^` as the legacy reference, especially `src/qml/EditCompositeLoop.qml`, `src/qml/DetailsPane.qml`, and `src/qml/CompositeLoop.qml`; port behavior and scheduling concepts, not QML/Qt code or dependencies.
- Keep `shoop_app_api` snapshots immutable and controller-independent. Publish semantic track/event data (IDs, labels, start/end times, kind), while keeping pixel geometry, zoom, scroll state, and swimlane assignment inside `shoop_egui`.
- Preserve the complete canonical composition needed by the view instead of relying only on the current lossy `Vec<Vec<LoopId>>` projection. Existing composition control/playback behavior must remain unchanged; this milestone adds no GUI mutation intents.
- Match the legacy schedule rules in the current frame-domain model: each playlist starts at zero; parallel elements use the section base plus their own delay; the next serial section follows the longest delayed element; natural loop span or an explicit cycle override determines each end; independent playlists may overlap. Handle empty/zero-length input and arithmetic limits without panic.
- Preserve main-track ordering and loop/track names from the authoritative application model. Do not recursively expand a referenced composite inside its rectangle.
- Make interval packing a pure deterministic helper. Sort/tie-break explicitly so snapshot map ordering cannot cause visual jitter.
- Keep rendering proportional to visible rows/events where practical, clip off-screen painting, and do not put session/media payloads or GUI objects into snapshots.
- Reuse the existing details-pane styling and control-safe scroll behavior. Keep the timeline readable at the pane’s minimum and maximum sizes; no visual redesign of unrelated widgets.
- Verification is headless/unit/command-line only. Do not use screenshots, GUI automation, or an interactive application session; the user owns the final visual assessment.

## Staged implementation plan

### Stage 0 — Baseline and contract

- [ ] Commit the approved plan, update the branch from current `origin/master`, confirm a clean worktree, and run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_app_api -p shoop_app -p shoop_egui -- --test-threads=1` as the targeted baseline.
- [ ] Re-read the legacy QML scheduling/layout paths and lock representative fixtures for serial, parallel, delayed, forced-duration, multi-playlist, script-kind, empty, and same-track-overlap cases.
- [ ] Keep this plan updated as evidence changes; do not alter goals or acceptance criteria without explicit user approval.

**Verification:** baseline targeted tests pass and the selected fixtures have documented expected start/end/track placement derived from the QML behavior.

### Stage 1 — Canonical model and snapshot projection

- [ ] Retain a complete internal composite representation across session load, key/MIDI/Lua composition updates, clear, and save, while preserving the existing playback projection and behavior.
- [ ] Add controller-independent composite details types to `shoop_app_api` and attach an optional composite payload to `LoopDetailsState`.
- [ ] In `shoop_app`, project all playlists into scheduled event spans, resolve source loop names and source track IDs/names, preserve main-track order and composite kind, and publish this only for a singly selected composite.
- [ ] Ensure primitive details still publish media/loading state unchanged, and empty composites publish an empty composite payload rather than primitive “no data.”
- [ ] Add model tests proving regular and script snapshots, all-playlist/parallel/serial timing, overlap inputs, empty schedules, composition updates, and save/load preservation.
- [ ] Commit the completed data-model/snapshot milestone.

**Verification:** targeted `shoop_app_api`/`shoop_app` tests demonstrate exact fixture rows and event spans and existing composition playback/persistence tests remain green.

### Stage 2 — Deterministic swimlane layout

- [ ] Add a dedicated egui composite timeline module with a pure interval-packing helper that groups by track and assigns the lowest reusable lane using explicit event tie-breaks.
- [ ] Cover true overlap, containment, equal starts, touching boundaries, duplicate spans from independent playlists, empty tracks, and stable repeated layout in unit tests.
- [ ] Derive per-track height and total timeline extent from the packed result without embedding view geometry in the application snapshot.
- [ ] Commit the layout milestone.

**Verification:** focused `shoop_egui` tests assert lane indices/counts and row growth for all edge cases.

### Stage 3 — Details-pane rendering, zoom, and scrolling

- [ ] Route composite details to the new timeline before the primitive audio/MIDI empty-state path in `details_pane.rs`.
- [ ] Render a compact read-only header/kind indicator, fixed track labels, track backgrounds, and clipped named event rectangles at their packed timespans.
- [ ] Add bounded horizontal zoom with a useful default and fit/reset behavior, retaining per-selected-composite view state and resetting/clamping it safely when selection or extent changes.
- [ ] Add coordinated horizontal and vertical overflow scrolling without intercepting control-modified input intended for application shortcuts.
- [ ] Add headless egui tests at narrow/wide pane sizes that verify composite dispatch, painted named events, overlap-driven row height, zoom scale changes, overflow extent, empty messaging, and primitive-details regression.
- [ ] Update the loop-details documentation to describe regular/script composite viewing and explicitly state that editing is deferred.
- [ ] Commit the rendering milestone.

**Verification:** targeted `shoop_egui` tests pass without launching the app, including native and `wasm32` compilation of the new path.

### Stage 4 — End-to-end non-interactive validation

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo test --locked --no-default-features -p shoop_audio_protocol -p shoop_audio_worklet -p shoop_egui -p shoopdaloop`, `RUSTFLAGS="-D warnings" cargo build --locked --no-default-features -p shoopdaloop --target wasm32-unknown-unknown`, and `RUSTFLAGS="-D warnings" cargo build --locked -p shoop_audio_worklet --target wasm32-unknown-unknown`; do not launch a browser or GUI session.
- [ ] Review the final diff for unrelated formatting, accidental editor controls, snapshot payload growth, and divergence between documented and tested schedule semantics.
- [ ] Commit any validation fixes as a separate meaningful milestone and rerun every affected gate.

**Verification:** all listed local gates are green and the worktree contains only intended plan/feature/documentation changes.

### Stage 5 — Pull request and three-green CI handoff

- [ ] Push the implementation branch to `origin` and open a non-draft PR against `master` summarizing scope, legacy-QML reference, automated evidence, deferred editing features, and the user-owned manual look-and-feel check.
- [ ] Watch every required PR check to completion. Diagnose each failure from logs, reproduce locally where defensible (using `.agents/info/ci-repro.md` for contention/flakes), fix code-owned failures, push, and rerun affected local gates; do not count cancelled/skipped-required/red runs as green.
- [ ] After the final push, obtain three sequential complete green required-check sets for the same head SHA, rerunning the complete CI workflow sequentially as needed. Reset the count to zero after any failed run or code change, and record the final SHA plus all three run URLs/IDs in this plan or the PR.
- [ ] Only after the third consecutive green run, report the PR URL, final SHA, local validation results, three CI runs, known non-visual limitations, and concise manual inspection steps to the user.

**Verification:** the PR is open, its final head SHA is unchanged across three consecutive complete green CI runs, and no interactive GUI look-and-feel claim is made.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
