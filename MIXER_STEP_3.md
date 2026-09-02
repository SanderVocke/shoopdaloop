# Mixer step 3: user-managed buses and focused Connections views

## Goal

Implement the next increment of `MIXER_ARCHITECTURE.md`: replace the fixed-only Master capability with a user-managed collection of named audio buses, each with a user-chosen positive channel count, and let users add, remove, and visually reorder those buses. Persist the visual order without giving it any routing, DSP, control, identity, or Lua-selection meaning.

Split the Connections dialog into two focused tabs:

```text
Tracks tab, bus destination mode:
System inputs -> Tracks -> Buses

Tracks tab, direct destination mode:
System inputs -> Tracks -> System outputs

Bus outputs tab:
Buses -> System outputs
```

The Tracks tab has one presentation toggle between `Buses` and `System outputs`; those destination columns are never shown together. Changing tabs or destination mode only changes what is visible. It must not create, remove, or reinterpret any connection.

This increment generalizes the completed fixed-Master routing and control work while retaining the bounded post-track-output graph, explicit routes, post-sum controls, backend authority, target parity, persistence, and realtime guarantees in `MIXER_ARCHITECTURE.md`.

## Scope

### Included

- A user-facing add-bus flow that accepts a bus name and any positive channel count allowed by documented checked resource limits.
- A disconnected, neutral new bus with stable bus, channel, and output-port identities and deterministic channel labels.
- Removal of any bus, including the initially created Master; zero buses is a valid runtime and persisted state.
- Transactional cleanup of a removed bus's track-to-bus routes, bus-to-system links, pending mutations, controls, meters, and UI state.
- Visual drag reordering of buses in the sidebar and Connections views, persisted independently from semantic bus identity/order.
- Existing gain, stereo-only balance, mute, and post-processing peak behavior for every bus and arbitrary channel count.
- Native, dummy, fake, Worker, AudioWorklet, worklet-client, application, session, UI, and existing Lua-control parity.
- A two-tab Connections dialog with the mutually exclusive Tracks-tab destination modes described above.
- Session-version migration from the fixed-Master document to multiple user-managed buses and explicit display order.
- Documentation, deterministic native and WebAssembly tests, packaged-browser validation, a new unified PR against `master`, green exact-head CI, and closure of all automated Codex review findings.

### Excluded

- Bus-to-bus routing or any new route shape.
- Track inputs, loops, monitoring, dry/wet internals, MIDI, or synchronization as mixer graph entities.
- Resizing or renaming an existing bus after creation; a different shape or name requires creating a replacement bus and explicitly reconnecting it.
- Per-channel bus gain/mute, route/send levels, solo, pre-fader sends, implicit channel mapping, or automatic rerouting.
- Editable bus insert chains, plugin hosting, latency compensation, recording/export sinks, or offline rendering.
- Lua APIs for creating, removing, renaming, resizing, or visually reordering buses.
- Persisting the selected Connections tab or destination-view toggle; they are dialog presentation state.
- Treating the Connections destination toggle as a mutually exclusive routing policy. Direct and bus-mediated routes may coexist even though the first tab never displays both destination sets simultaneously.

## Immutable acceptance criteria

1. A new session still starts with one disconnected stereo bus named `Master`, with ordered `Left` and `Right` channels and neutral controls. Master is an ordinary user-managed bus in this increment: it may be removed, and a saved session with zero buses reloads with zero buses rather than silently recreating it.
2. The user can add a bus from the main bus UI by supplying a non-empty bounded UTF-8 display name and a positive channel count. Mono, stereo, and channel counts greater than two are supported up to explicit checked resource/transport limits; unsupported or excessive requests fail visibly and atomically rather than being clamped or partially created.
3. A newly added bus has globally unique stable bus, channel, and output-port identities; deterministic ordered labels (`Mono`, `Left`/`Right`, or `Channel N`); `0 dB` gain, centered balance storage, unmuted state, meter-floor peaks, no incoming mixer routes, and no system-output links. Duplicate display names are allowed and never serve as identity.
4. The user can remove any current bus through a confirmation-gated action. Successful removal atomically eliminates that bus, its channels and output ports, all incoming mixer routes, all bus-to-system links, and associated pending/error/control/meter state. Rejection, saturation, timeout, stale identity, or backend failure leaves the previously confirmed bus and graph authoritative and usable.
5. The user can reorder buses by stable identity in the sidebar. The same visual order is used wherever buses are listed in the UI and round-trips exactly through session save/load, replacement, resampling, and compatible driver switching. Reordering sends no backend topology/control/route mutation, changes no stable identity, audio, connection, control, meter association, or existing Lua bus selector, and remains correct with duplicate names.
6. Every bus retains the step-2 processing contract: `sum -> gain/stereo balance/mute -> post-processing meter -> output fan-out`. Gain and mute apply to arbitrary channel counts, balance is available only for exactly two ordered channels, and add/remove/reorder never creates an implicit route.
7. Bus creation and removal preserve the architecture's control-plane/realtime-plane split. Validation, host-port work, allocation, graph construction, and complete schedule preparation happen off the callback; the matching graph and active route table change atomically; the prior graph remains active on failure; and the callback performs no allocation, blocking, locking, logging, or unbounded work.
8. The Connections dialog has exactly two routing tabs. The Tracks tab shows `System inputs -> Tracks` plus exactly one selected output destination: either bus input facets or system outputs. The Bus outputs tab shows only bus output facets and compatible system outputs. Bus input and output facets are never conflated even when represented by one channel row in different views.
9. On the Tracks tab, `Buses` and `System outputs` are mutually exclusive at the endpoint, route, pending, error, hit-testing, and drag-target levels. Switching the destination toggle or either tab cancels stale drags but does not mutate confirmed or pending links; hidden routes reappear with their exact authoritative state when their view is selected.
10. Existing host-boundary connection capabilities remain reachable: system-to-track/application inputs continue to support the established Audio/MIDI and track filters, direct track-to-system routing remains available in direct mode, track-to-bus routing remains available in bus mode, and bus-to-system routing remains available on the second tab. Per-track dialog scope filters the Tracks tab without changing or hiding the globally applicable bus-output tab's state.
11. Backend and application snapshots remain authoritative for structural bus operations and routing. Native, dummy/engine, fake, Worker, AudioWorklet, and worklet-client paths expose equivalent create/remove identities, snapshots, failures, command saturation, replacement, reconnect/replay, and cleanup behavior; optimistic UI state always settles or rolls back in bounded time.
12. The session document has an explicit new version whose canonical representation supports zero or more buses, arbitrary positive channel counts, exact stable identities, controls, explicit routes/host links, and a display-order permutation containing every bus identity exactly once. Version-10 fixed-Master sessions migrate without audible or identity changes and initialize display order to Master; malformed names, counts, IDs, channel/output shapes, order permutations, controls, routes, and limits are rejected before mutation.
13. Lua API version 1.6 extends the existing bus control APIs to every current bus without using visual order as selector order. Zero-based selectors enumerate ascending stable `BusId`, so Master remains index `0` while it exists, visual reordering cannot retarget a script, and missing buses retain the established `nil`/no-selection behavior. Scripts targeting compatible older minor versions continue to run, and no bus-management Lua API is added.
14. The completed work is committed on a new `shoopdaloop-mixer-step-3` branch descended from the final step-2 head and pushed as a new non-draft unified PR directly against current `master`. Its diff contains the complete bounded mixer foundation plus steps 2 and 3, thereby replacing PR #843 without stacking or retargeting either PR. At completion PR #843 is closed as superseded with a link to the new PR; the replacement PR is merge-clean, every required CI check is green on its exact final head, every automated Codex finding has an evidence-backed reply/fix, and a fresh Codex review reports no major issues on that same head.

Acceptance criteria may not be weakened by hard-coding a small set of channel counts, preserving visual order only in widget memory, silently recreating Master, hiding rather than retaining authoritative links, treating a green proxy test as target parity, or relying on CI/review results from an earlier SHA.

## Design rules and constraints

- Preserve every graph boundary and permitted/forbidden route shape in `MIXER_ARCHITECTURE.md`: only track output to bus channel, track output to system sink, and bus output to system sink are valid.
- Treat the initial Master as new-session application policy, not a reserved engine shape or immortal identity. After migration, all buses use the same normalized implementation and lifecycle rules.
- Use typed stable identities for buses, channels, output ports, routes, controls, pending operations, and display-order entries. Names, labels, list indices, host names, and runtime arena positions are never identity.
- Keep semantic bus enumeration separate from visual order. The application/UI may hold an explicit ordered `BusId` permutation, but backend scheduling, route resolution, persistence validation, and Lua selector ordering must not derive semantics from that permutation.
- Use the session bus display-order field solely for presentation. Serialize bus entities canonically by stable identity and validate that display order contains every current bus exactly once with no unknown or duplicate ID.
- Accept any positive channel count that fits explicit checked application, graph, protocol, host-port, and session budgets. Do not introduce a mono/stereo-only branch or silently truncate channels. Establish and document the concrete limits before implementation depends on them.
- Generate channel presentation labels deterministically at creation: `Mono` for one channel, `Left` and `Right` for two, and `Channel 1` through `Channel N` otherwise. Labels persist as metadata and have no routing meaning.
- Generate host-safe output-port names from stable identities rather than user display names, so duplicate names and display changes cannot collide or retarget external links.
- Create buses disconnected with neutral controls. Removal is the only management operation allowed to cascade route/link deletion, and it must expose that consequence in the confirmation UI.
- Model create/remove as typed bounded backend operations, not as UI-only state and not as an implicit full-session reload. Prepare and publish topology changes according to the existing desired/active graph transaction.
- Keep the old schedule and matching route table active until the complete new schedule is ready. Never publish a bus or deletion as confirmed before the corresponding backend graph and host-port inventory are authoritative.
- Define command replay/supersession deliberately: create/remove must not be lost behind control or route commands, removed buses and their commands must not resurrect after Worker/AudioWorklet restart, and replay must reconstruct only the latest confirmed logical topology.
- Preserve existing bus controls and meters by channel identity across unrelated bus additions, removals, reorders, snapshots, driver switches, and session replacement. Reordering must not reset widget-local fader, dial, or meter animation state.
- Build Connections endpoint sets and route visibility from an explicit tab/view model. Do not construct the current five-column graph and merely cover a column visually; hidden destinations must be absent from layout, interaction, and route painting while their authoritative state remains untouched.
- In the Tracks tab, present bus channels as sink facets only. In the Bus outputs tab, present them as source facets only. Keep direct host links and mixer links typed and independently authoritative.
- Buses remain audio-only. MIDI system inputs and application/track ports retain their current direct host-connection behavior and filters; the bus destination mode must not invent MIDI bus targets.
- Bump the audio protocol and raw host contract for structural bus commands, the session document for general buses/display order, and Lua to API 1.6 for multi-bus selector semantics; update fixtures and compatibility documentation in the same stages.
- Preserve deterministic serialization, collision-aware ID allocation, structural sharing where practical, bounded queues and journals, and cancellation/rollback during session replacement or driver switching.
- Add no bus-to-bus routes, automatic track-to-bus routes, automatic bus-to-system routes, route gain, solo, or editable processing.

## Implementation stages

Dependencies are linear unless a stage explicitly says otherwise. Each stage must leave touched packages compiling and focused tests passing before dependent work starts.

### Stage 0 — Establish the unified replacement branch and baseline

- [ ] Record the final exact head, merge base, all-green checks, and no-major-issues Codex result of PR #843; confirm `MIXER_STEP_2.md` and `MIXER_MASTER_GOAL.md` have no unchecked items.
- [ ] Create `shoopdaloop-mixer-step-3` from the final step-2 head, not from `master`, so the new branch contains the complete prior mixer implementation. Keep its eventual PR base as `master`.
- [ ] Verify the initial `origin/master...HEAD` diff is exactly the completed mixer foundation/step-2 work plus this plan, with no unrelated changes.
- [ ] Inventory fixed-Master assumptions across backend, native runtime, engine graph, application model, protocol/worklet replay, session validation, Lua snapshots, sidebar, Connections graph, and documentation before replacing them.
- [ ] Record the concrete name-length, bus-count, per-bus channel-count, total-channel/output-port, graph-capacity, command-capacity, and session-budget limits selected from existing bounded infrastructure. Any newly necessary limit must be justified as a resource bound rather than a functional mono/stereo restriction.
- [ ] Record the resolved lifecycle semantics from the immutable criteria: Master is removable, zero buses persist, names are chosen only at creation and may duplicate, existing buses are not resized, visual order is separate from Lua/backend order, and Connections modes are presentation-only.

Verification:

- [ ] Confirm the branch points at the recorded step-2 SHA and its merge base with `master` is understood before implementation.
- [ ] Run the existing fixed-Master routing, control, session, sidebar, Connections, Lua, and native/browser focused tests as the regression baseline.

### Stage 1 — Define dynamic bus lifecycle, identity, order, and protocol contracts

Depends on Stage 0.

- [ ] Add normalized typed bus-creation and bus-removal requests/results/failures to the backend boundary, including stable channel/output mappings and explicit capacity/validation errors.
- [ ] Extend application APIs with typed create, remove, and `MoveBefore(Option<BusId>)` intents plus structural pending/error state; keep gain/balance/mute actions keyed by `BusId`.
- [ ] Add an explicit application display-order model independent of the backend bus map and semantic/Lua selector order.
- [ ] Replace fixed Master constants in general contracts with a default-bus constructor policy while retaining reserved legacy IDs only for migration compatibility.
- [ ] Bump the audio protocol and add bounded create/remove commands, acknowledgements/failures, snapshots, stable mapping data, and journal classifications needed by remote backends.
- [ ] Update fake backend operation capture and configurable failures so tests can prove exact definitions, IDs, ordering isolation, rejection, saturation, and rollback.

Verification:

- [ ] Add contract tests for valid mono/stereo/multichannel definitions, deterministic labels, duplicate names, empty/oversized names, zero/excessive counts, identity collision/exhaustion, stale remove, and exact fake operations.
- [ ] Prove display moves accept only current stable IDs, handle self/end moves deterministically, and emit no backend operation.
- [ ] Run focused `shoop_backend`, `shoop_app_api`, `shoop_audio_protocol`, and fake-backend tests.

### Stage 2 — Generalize engine and native backends to arbitrary bus collections

Depends on Stage 1.

- [ ] Replace single `master_bus` runtime storage with stable bus collections whose channels own independent summing inputs, processed outputs, controls, peaks, and host-visible output descriptors.
- [ ] Implement off-realtime bus creation that validates and reserves every ID/capacity first, creates all channels/output ports, prepares a complete schedule, and publishes the bus only after matching graph activation.
- [ ] Implement transactional removal that prepares a graph without the bus and its incoming routes, removes/disconnects its output host links safely, and publishes one matching bus/route/port snapshot without affecting other buses or direct track outputs.
- [ ] Generalize session capture/replacement, driver restart, graph scheduling, route validation, polling, control application, and metering from one stereo Master to zero or more arbitrary-channel buses.
- [ ] Preserve stereo balance only for exactly two channels and uniform gain/mute for all other counts; zero buses must process as a valid no-op topology.
- [ ] Extend preallocation, graph-capacity accounting, deferred destruction, realtime guards, and no-allocation tests for add/remove and simultaneous active multi-bus fan-in/fan-out.

Verification:

- [ ] Add deterministic engine/dummy and native tests creating mono, stereo, and greater-than-two-channel buses; route multiple tracks independently; verify summing, controls, peaks, direct fan-out, and cross-bus isolation.
- [ ] Prove successful add/remove changes the graph exactly once, failed preparation leaves the prior graph/routes/ports unchanged, and removing one bus cannot disturb another bus or direct link.
- [ ] Prove every callback path remains allocation/lock/log free with several active buses and after control-plane topology swaps.
- [ ] Run focused engine, native app-backend, port-meter, graph transaction, session replacement, and realtime no-allocation suites.

### Stage 3 — Implement Worker, AudioWorklet, and remote replay parity

Depends on Stages 1–2 and may proceed alongside Stage 4 after contracts stabilize.

- [ ] Dispatch protocol create/remove commands in Worker and AudioWorklet runtimes through the same normalized backend operations and publish complete authoritative bus/channel/output mappings.
- [ ] Generalize worklet-client conversion and detached/bootstrap snapshots from fixed Master seeding to the current confirmed bus collection, including a valid zero-bus state.
- [ ] Update bounded command reservation and supersession so structural bus commands retain headroom, controls/routes cannot overtake creation, and removal prunes obsolete queued and durable commands for that bus.
- [ ] Update reconnect/replay state transactionally so only currently confirmed buses, controls, mixer routes, and host links are reconstructed; removed buses never resurrect and failed create/remove restores the prior journal.
- [ ] Generalize browser session replacement and cancellation mappings for arbitrary bus/channel counts without unbounded event queues or monolithic replay submission.
- [ ] Update raw Wasm host contracts, wire fixtures, snapshot fixtures, and protocol-version checks.

Verification:

- [ ] Add native-harness and `wasm32-unknown-unknown` protocol/worklet tests for multi-bus create/remove, stale IDs, malformed shapes, saturation, out-of-order responses, cancellation, restart/replay, and zero buses.
- [ ] Run AudioWorklet DSP fixtures with mono, stereo, and multichannel buses and verify exact routing/control/meter behavior before and after removing one bus.
- [ ] Repeat large-session/replay tests with many buses/channels to prove bounded pending commands, event draining, memory limits, and mandatory attach/recovery headroom.
- [ ] Run focused `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_worklet_client`, and remote-application suites in native and browser runtimes.

### Stage 4 — Add authoritative application lifecycle and visual-order state

Depends on Stages 1–3.

- [ ] Allocate collision-safe persistent source identities for a complete new bus definition, submit one normalized create request, and represent creation as pending until the backend snapshot supplies its complete mapping.
- [ ] Reconcile create/remove on confirmation, rejection, saturation, timeout, stale identity, backend replacement, and disappearing/reappearing generations without publishing false structural truth.
- [ ] On confirmed removal, prune desired controls, pending mixer/host operations, failures, channel/output mappings, widget state, and display order for only that bus; preserve unrelated pending work.
- [ ] Implement stable-identity visual `MoveBefore` ordering entirely in the application/UI state with deterministic behavior under concurrent backend snapshots and bus disappearance.
- [ ] Keep application bus snapshots in display order for presentation while constructing Lua control snapshots in a separate deterministic stable-identity order.
- [ ] Reject or defer save, driver switch, and session replacement at unsafe structural-operation boundaries according to the existing bounded task-state contract.

Verification:

- [ ] Add application tests for create/remove pending-to-confirmed flow, failures, timeout compensation, late confirmation, saturation, stale mappings, duplicate names, identity exhaustion, backend recreation, and cleanup isolation.
- [ ] Prove repeated reorder operations never call the backend, mutate routing/control state, reset meters, or alter Lua-selected bus identity.
- [ ] Verify immutable snapshot structural sharing remains correct for unchanged buses while topology, meter, control, and visual-order updates remain visible.
- [ ] Run focused `shoop_app`, `shoop_app_api`, fake-backend, native replacement, and remote-application tests.

### Stage 5 — Persist arbitrary buses and visual order

Depends on Stage 4.

- [ ] Bump `SESSION_DOCUMENT_VERSION` and define canonical zero-or-more bus records plus an explicit bus-display-order identity permutation; serialize bus entities independently from their visual order.
- [ ] Generalize codec validation from exactly one fixed stereo Master to arbitrary positive channel counts, globally unique bus/channel/output IDs, canonical audio-output port shapes, finite valid controls, explicit routes, and checked aggregate limits.
- [ ] Migrate version-10 sessions to the identical Master bus, channel/output identities, controls, routes, external links, and one-element display order without adding an audible path.
- [ ] Preserve zero buses as intentional current-format state; retain the historical policy that older pre-mixer sessions receive the disconnected default Master during their existing migration chain.
- [ ] Capture and restore display order, bus names/shapes/controls, explicit mixer routes, and exact host links transactionally across ordinary load, resampling, browser transfer, and compatible driver switching.
- [ ] Reject missing/duplicate/unknown order entries, zero-channel buses, malformed labels/ports, ID collisions, unsupported controls, stale routes, excessive resources, and partial mappings before backend mutation.

Verification:

- [ ] Add deterministic archive round trips for zero, one, duplicate-named, reordered, mono/stereo, and multichannel bus sets with controls and both route classes.
- [ ] Add version-10 migration, malformed document, resource-limit, failed replacement rollback, cancellation, same-rate/resampled, driver-switch, and browser-transfer tests.
- [ ] Prove visual order changes archive metadata deterministically but does not change canonical bus definitions, route identity, Lua selector order, or audio results.
- [ ] Run focused `shoop_session`, archive, application save/load, backend replacement, and Wasm session suites.

### Stage 6 — Add sidebar bus management and drag ordering

Depends on Stages 4–5.

- [ ] Add an always-reachable bus-management affordance in the right sidebar that opens a bounded add dialog for name and positive channel count, with inline validation and visible pending/error state.
- [ ] Extend each bus block with a stable-identity drag handle and remove action while retaining name, channel-aware meter, mute, gain, and stereo-only balance controls.
- [ ] Require explicit removal confirmation that states the number of incoming mixer routes and outgoing system links that will be removed; disable duplicate submission while structural work is pending.
- [ ] Implement touch-safe drag reordering and autoscroll without stealing fader/dial/mute interactions or deriving drag identity from name/index.
- [ ] Preserve the fixed logo and sync-track layout, keep add/remove reachable with zero or many buses and short windows, and prune only state belonging to confirmed removed identities.

Verification:

- [ ] Add egui tests for valid/invalid add submission, mono/stereo/multichannel blocks, duplicate names, add failure, removal confirmation/cancellation/failure, zero-bus UI, pending disabling, and exact typed intents.
- [ ] Add reorder tests across duplicate names, scrolling, short windows, touch/mouse input, backend snapshot refresh, bus removal during drag, and retained control/meter widget state.
- [ ] Verify add creates no routes, removal advertises and removes only its own links, and reordering changes only presentation order.
- [ ] Run focused `shoop_egui` bus/sidebar/AppWidget tests and native/browser rendering smokes.

### Stage 7 — Split the Connections dialog into focused routing tabs

Depends on Stage 4 and may proceed alongside Stage 6.

- [ ] Introduce explicit `Tracks` and `Bus outputs` tab state and a Tracks-tab `Buses`/`System outputs` destination mode; clear drag state whenever tab, mode, filters, scope, or authoritative revision invalidates it.
- [ ] Refactor graph construction to receive the selected view and produce only allowed columns/facets/routes rather than building and visually masking the five-column graph.
- [ ] In Tracks/bus mode, expose system-input sources, existing track/application input sinks, eligible track output sources, and bus input sinks only.
- [ ] In Tracks/direct mode, expose system-input sources, existing track/application input sinks, eligible track output sources, and compatible system-output sinks only.
- [ ] In Bus outputs, expose bus output sources in persisted visual order and compatible system-output sinks only; do not expose track endpoints or bus input connectors.
- [ ] Preserve authoritative confirmed/pending/error state while hidden, exact typed route intents, filtering, per-track scope semantics, line interaction, clipping, window sizing, and backend availability/error presentation.
- [ ] Keep Audio/MIDI behavior coherent: buses remain audio-only, MIDI input/direct routing remains available in the applicable Tracks view, and no MIDI route is presented as bus-compatible.

Verification:

- [ ] Add graph-model tests asserting exact endpoint/route sets for all three visible layouts and proving buses and system outputs never coexist on the Tracks tab, including pending/error cases.
- [ ] Add interaction tests for tab/mode switching, stale-drag cancellation, route creation/disconnection in each view, hidden-route retention/reappearance, duplicate bus names, visual bus order, filters, and per-track scope.
- [ ] Verify existing direct, mixer, bus-output, additive fan-in/fan-out, MIDI/global-control, authority, clipping, and no-overlap behavior does not regress.
- [ ] Run focused Connections-dialog, AppWidget, native headless, Chromium, and packaged-browser tests.

### Stage 8 — Preserve Lua behavior and update documentation

Depends on Stages 4–7.

- [ ] Generalize the existing Lua control snapshot to every current bus in deterministic stable-identity order independent of visual order; retain zero-based scalar/list/`nil` conventions and stereo balance validation.
- [ ] Bump the Lua API minor version to 1.6 for deterministic multi-bus selector semantics, preserve acceptance of scripts targeting older compatible minors, and add no bus-management functions.
- [ ] Add Lua tests proving controls target the same identities before/after visual reorder, work with mono/stereo/multichannel buses, handle removal/zero buses, and reconcile backend rejection.
- [ ] Update `MIXER_ARCHITECTURE.md` with the completed user-managed-bus and focused-Connections increment while keeping sandbox/control increments as historical stages.
- [ ] Update `docs/port_model.md`, `docs/session_format_v1.md`, `docs/lua_compatibility_contract.md`, relevant scripting/UI developer documentation, `src/rust/shoopdaloop/README.md`, raw host documentation, and tracing inventory for the new lifecycle, order, migration, views, limits, and target parity.

Verification:

- [ ] Run focused `shoop_scripting`, `shoop_app`, script-resource, native runtime, and Wasm scripting tests.
- [ ] Audit every user-facing label/diagram and compatibility statement against implemented behavior, especially “System inputs/outputs,” visual-only order, removable Master, zero buses, and presentation-only Connections modes.

### Stage 9 — End-to-end validation and completion audit

Depends on all implementation stages.

- [ ] Run an end-to-end dummy/native scenario: create mono, stereo, and multichannel buses; retain direct track outputs; create explicit track-to-bus and bus-to-system routes; exercise controls/meters; reorder; save/reload; switch drivers; remove a routed bus; and verify exact identities, order, cleanup, direct audio, unaffected buses, and zero-bus behavior.
- [ ] Run the equivalent Worker/AudioWorklet scenario, including restart/replay and session replacement, and verify authoritative snapshots/audio before and after every structural operation.
- [ ] Exercise all three Connections layouts and prove switching tabs/modes neither mutates nor loses hidden confirmed/pending routes.
- [ ] Verify new, version-10-migrated, zero-bus, duplicate-name, reordered, resampled, maximum-supported, over-limit, malformed, cancelled, and failed-replacement sessions.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace` in the environment selected by `.agents/info/build.md`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py` because Rust tests will change.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Build `shoopdaloop --no-default-features --target wasm32-unknown-unknown` and `shoop_audio_worklet --target wasm32-unknown-unknown --release`.
- [ ] Run the complete Node Wasm suite with `python3 scripts/run_wasm_tests.py --runtime node --profile dev`, relevant Chromium suites, `trunk build`, smoke-budget checks, and packaged Chromium/Firefox smokes when available.
- [ ] Run native headless startup/rendering validation and any real-host tests available without weakening unavailable-host behavior into success.
- [ ] Build a prompt-to-artifact checklist mapping every scope bullet, immutable criterion, named file/contract, command, test, target, migration, PR requirement, and Codex finding to concrete exact-head evidence; inspect the evidence and leave no unchecked or weakly covered requirement.

### Stage 10 — New unified PR, CI, review, and replacement closure

Depends on Stage 9 local gates.

- [ ] Ensure every completed stage or meaningful milestone has a focused commit, the worktree is clean, and `origin/master...HEAD` contains only the complete mixer foundation plus steps 2 and 3.
- [ ] Push `shoopdaloop-mixer-step-3` and open a new non-draft PR directly against `master`; clearly state that it replaces #843 and include architecture boundaries, lifecycle/order semantics, Connections layouts, version changes, and exact local verification results.
- [ ] Fetch current `master`, rebase the unified branch when needed, resolve conflicts without weakening acceptance criteria, rerun affected gates, and verify the new PR remains merge-clean and contains all prior mixer work. Never retarget it to #843 or another feature branch.
- [ ] Monitor `gh pr checks` for the exact current head SHA. For every failure, inspect run attempt, matrix job, logs, and artifacts using `.agents/info/ci-debug.md`; reproduce deterministic failures locally and use `.agents/info/ci-repro.md` before classifying timing behavior.
- [ ] Fix every real CI defect, rerun affected local suites, commit/push, and restart the exact-head audit. Do not rely on checks from an earlier SHA, omitted matrix jobs, or proxy green statuses.
- [ ] Enumerate every root automated Codex finding, assess it against architecture and code, implement each valid fix with focused regression coverage, and reply with commit/evidence. Reply to invalid findings with concrete evidence rather than silently dismissing them.
- [ ] Request fresh Codex review after each fix batch and continue until the exact final head has no unresolved findings and an explicit no-major-issues result.
- [ ] Once the replacement PR itself is complete and exact-head green/reviewed, close PR #843 as superseded with a link to the replacement; verify #843 was not merged or used as the replacement PR's base.
- [ ] Perform the final audit: local/remote heads match, worktree is clean, merge base is current `master`, replacement PR is open/non-draft/merge-clean, all required checks are successful or legitimately skipped, all findings have evidence-backed replies, latest Codex review covers the exact SHA, old PR is closed as superseded, and every plan checkbox/criterion has concrete evidence.

Verification:

- [ ] Record the final branch SHA, replacement PR URL, current `master`/merge-base SHA, all-green exact-head check rollup, Codex result and finding/reply counts, #843 closure link, and full prompt-to-artifact audit before declaring step 3 complete.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
