# Stereo Master bus sandbox implementation plan

## Goal

Implement the first post-track-output mixer slice described in `MIXER_ARCHITECTURE.md`: every session has one fixed stereo **Master** bus, initially routed from no tracks and to no system sinks, and users can manage track-output-to-Master and Master-to-system routes in the Connections dialog while retaining direct track-to-system routing.

The implementation must establish the durable mixer identities, authoritative route state, coherent realtime graph installation, backend/protocol parity, and persistence needed for later bus controls, additional buses, recording sinks, and bus processors without generalizing into track inputs or loop internals.

## Scope

### Included

- Audio-only post-track-output mixer concepts: bus, bus channel, and track-output-to-bus route.
- Exactly one application-created bus named **Master**, with two ordered channels labelled Left and Right.
- Unity-gain, per-channel routes from any track audio output to either Master input channel.
- Existing host connections from track outputs directly to system sinks.
- Host connections from Master outputs to compatible system sinks.
- Authoritative confirmed, pending, failed, connect, and disconnect behavior for mixer routes.
- Native, dummy/offline, browser Worker, and AudioWorklet behavior through the normalized backend and wire protocol.
- Session capture/replacement, save/load, and audio-driver switching for the Master bus and its routes.
- A Connections dialog bus column with a left input facet and right output facet for each Master channel.

### Excluded

- Track inputs, loop channels, dry sends, wet returns, MIDI, and monitoring in the mixer graph.
- Bus-to-bus routing.
- Adding, removing, renaming, or resizing buses through the UI.
- Bus or route gain, balance, mute, solo, metering, or smoothing controls.
- Bus processors, plugin hosting, latency compensation, recording/export sinks, and offline rendering.
- Implicit track-to-Master or Master-to-device routes.
- Changing existing direct track startup/autoconnection policy.

## Immutable acceptance criteria

1. Every new, loaded, and driver-switched application session exposes exactly one bus with stable application identity, the display name `Master`, and exactly two ordered audio channels, Left and Right.
2. A newly created or legacy-migrated session has no confirmed track-to-Master routes and no confirmed Master-to-system routes unless those routes were explicitly restored from persisted session state. Existing direct track-to-system behavior remains unchanged.
3. The Connections dialog presents the output path as ShoopDaLoop sources, Buses, and System sinks. Each Master channel accepts track outputs on its left and offers the corresponding Master output on its right. Direct track-to-system routes may bypass the bus column.
4. Users can connect and disconnect any track audio output to either Master channel, independently of the track's direct system links. Multiple tracks may feed one Master channel and one track output may feed both Master channels and system sinks simultaneously.
5. Users can connect and disconnect either Master output to compatible system sinks using the existing host-connection authority and pending/error semantics.
6. Confirmed mixer routes come only from backend/worklet snapshots. Pending requests settle on confirmation, explicit failure, disappearance, saturation, or timeout; the UI never treats its own drag state as routing truth.
7. Audio tests prove silence with no Master inputs, exact unity pass-through for one route, deterministic additive summing for multiple routes, fan-out without signal consumption, route removal, and continued direct track output.
8. A topology change never exposes a new route to an old processing schedule. Validation or schedule-build failure leaves the previous active route table and audible graph intact.
9. Master identity, channel identity, track-to-Master routes, and Master external links round-trip through session save/load and backend session replacement. Audio-driver switching preserves compatible desired routing using the existing transactional replacement model.
10. The implementation preserves realtime allocation/lock guarantees and passes native and WebAssembly protocol/runtime validation.
11. No UI or application intent permits creating, deleting, resizing, renaming, or adjusting the Master bus in this MVP.

Acceptance criteria may not be weakened by treating unsupported targets, failed route confirmation, or non-persisted state as expected sandbox limitations.

## Design rules and constraints

- Follow `MIXER_ARCHITECTURE.md`; mixer routing begins at final track audio outputs and never reaches into track internals.
- Keep existing application-to-host connection structures intact where practical. Add typed mixer-route state rather than encoding an application-to-application route as a fabricated host ID.
- Represent track-to-bus routing with stable typed identities. Names, channel labels, engine indices, and host registry IDs are not route identity.
- Model bus channels generically by ordered channel identity/count even though application capability is fixed to one stereo Master bus.
- Treat a bus channel's summing input and external-facing output as distinct facets. The UI may combine them into one row.
- Use explicit per-channel unity routes. Do not introduce implicit stereo mapping, panning, or attenuation.
- Retain direct track output ports as host-connectable sources. Routing a track to Master must not disconnect or duplicate ownership of its existing host connection.
- Keep desired topology separate from active callback topology. Install a prepared route table with the schedule that was built for it.
- Deduplicate identical routes and validate source/target existence, audio type, ownership, and permitted edge shape before activation.
- Publish mixer-route confirmation only after the matching prepared graph is active. Preserve the old confirmed route on failed replacement.
- Use one normalized semantic model across native and remote/browser adapters. Do not add target-specific mixer behavior to the application or UI.
- Persist explicit routes and explicit absence of routes. Legacy migration must preserve old direct output links and add a disconnected Master.
- Bump wire/session versions when their serialized contracts change; update compatibility fixtures and format documentation in the same stage.
- Do not add callback allocation, blocking, logging, graph construction, or unbounded route traversal. Reserve route/summing capacity on the control path and retain no-allocation coverage.
- Keep unrelated connection-dialog behavior intact: host inventory normalization, script-owned MIDI routes, Global FX MIDI warnings, filters, route hit-testing, and exact disconnect intents.

## Implementation stages

Dependencies are linear unless a stage explicitly says work can proceed in parallel. Each stage must leave its touched packages compiling and its focused tests passing before the next stage begins.

### Stage 1 — Define mixer contracts and harden internal route installation

- [ ] Add engine-level typed internal audio-link data with deduplication and explicit connect/disconnect operations; reject missing, self, and audio/MIDI-incompatible endpoints.
- [ ] Split desired internal links from the active callback link table. Include the active link table in `PreparedSchedule` or an equivalent prepared graph object and swap it atomically with its matching schedule.
- [ ] Ensure stale processing runs the complete previous graph: `propagate_port` must not read newly requested links before their schedule is installed.
- [ ] Make failed schedule construction leave active links untouched and keep the desired failure observable/recoverable rather than poisoning later callback execution.
- [ ] Add a reusable internal audio-port handle/factory to the application backend layer so buses do not depend on FX-chain-private port construction.
- [ ] Add engine tests for connect, duplicate connect, disconnect, fan-out, fan-in, coherent stale schedules, cycle/build failure rollback, tombstoned endpoints, and audio/MIDI mismatch.
- [ ] Extend realtime no-allocation tests to cover installed mixer fan-in and route changes between callbacks.

Verification:

- [ ] Run focused `shoop_engine` graph, session, app-backend, and no-allocation tests.
- [ ] Verify an intentionally delayed graph apply continues rendering only the old routes until the new prepared graph is installed.

### Stage 2 — Add normalized backend bus and mixer-route state

Depends on Stage 1.

- [ ] Introduce stable backend bus/channel identities and generic bus descriptions containing ordered audio channels and their input/output facets.
- [ ] Introduce a typed confirmed mixer-route key for `track audio output -> bus input channel`, plus normalized failures/revision state separate from external host links.
- [ ] Extend backend snapshots with buses and confirmed mixer routes; extend the backend trait with the control operation needed to request a unity track-to-bus connect/disconnect.
- [ ] Extend port ownership so Master output ports are bus-owned without pretending they belong to a track. Preserve existing track/global ownership behavior.
- [ ] Define one shared validator/lowering description for allowed mixer routes and fixed bus construction so native and engine backends cannot diverge semantically.
- [ ] Create a two-channel Master bus in `EngineBackend`: each channel has an internal summing input, a fixed owner-managed pass-through to a bus-owned external output, and no user route by default.
- [ ] Implement equivalent Master construction and route mutation in `NativeRuntime` using shared session/internal-port facilities.
- [ ] Confirm mixer routes only once the graph scheduler has installed the corresponding active route table; report rejected mutations through bounded backend failure state.
- [ ] Remove track-to-Master routes safely when a source track disappears; keep Master lifetime tied to the enclosing backend session.
- [ ] Add backend tests for Master shape/ownership, initial disconnection, source validation, duplicate/idempotent commands, summing, fan-out, disconnect, track removal, and coexistence with direct host links.

Verification:

- [ ] Run focused `shoop_backend` tests against engine/dummy and native app-backend implementations.
- [ ] Exercise a two-track deterministic signal fixture and assert direct outputs and Master outputs independently.

### Stage 3 — Carry the mixer contract through Worker and AudioWorklet

Depends on Stage 2 and may be developed alongside Stage 4 after backend types stabilize.

- [ ] Bump the audio protocol version and add wire bus/channel descriptions, confirmed mixer routes, route failures as needed, and a bounded set-mixer-route command.
- [ ] Define command journal/supersession behavior so repeated changes to one route coalesce without affecting another route.
- [ ] Extend AudioWorklet command handling and snapshots to use the same backend operation and authoritative active-route confirmation.
- [ ] Extend the worklet client to map wire identities and state into normalized backend bus/route snapshots and submit mixer-route commands.
- [ ] Update protocol serialization fixtures, command-capacity tests, worklet snapshot tests, remote application tests, and browser route/audio tests.

Verification:

- [ ] Build and test `shoop_audio_protocol`, `shoop_audio_worklet`, and `shoop_worklet_client` for their native test harnesses and `wasm32-unknown-unknown` targets.
- [ ] Verify command saturation and stale generations do not create false confirmed routes.

### Stage 4 — Add application bus identity, route reconciliation, and intents

Depends on Stage 2; wire-backed verification depends on Stage 3.

- [ ] Add stable application `BusId`/channel identity and immutable bus state to `shoop_app_api`; expose buses in `AppState` without adding edit controls.
- [ ] Add typed application mixer-route confirmed/pending/error state and a connect/disconnect intent whose endpoints cannot represent forbidden graph edges.
- [ ] Map backend Master and channel identities transactionally into application identities, including backend recreation during session replacement and driver switching.
- [ ] Register bus-owned external output ports without requiring a track owner. Keep bus input facets in mixer state rather than host-port candidate state.
- [ ] Reconcile pending mixer requests against authoritative backend snapshots with the same bounded timeout, saturation, stale endpoint, and error-reporting standards as host links.
- [ ] Preserve structurally shared immutable snapshots and revision changes only when visible bus/route state changes.
- [ ] Route application intents to the backend and reject stale track outputs, stale buses/channels, MIDI sources, and unavailable backends before mutation.
- [ ] Add application tests for initial Master state, identity stability, pending-to-confirmed and pending-to-error transitions, timeout, source-track removal, snapshot sharing, and remote backend parity.

Verification:

- [ ] Run focused `shoop_app_api`, `shoop_app`, fake-backend, and remote-application tests.

### Stage 5 — Extend the Connections dialog with bus facets

Depends on Stage 4.

- [ ] Refactor connection-graph presentation so source/sink capability belongs to an endpoint facet rather than to an entire column.
- [ ] Add a `Buses` column between ShoopDaLoop sources and System sinks. Present each Master channel as one grouped row with a left mixer-input connector and right bus-output connector.
- [ ] Render confirmed and pending track-to-Master curves from normalized mixer-route state while retaining existing application-to-host curves.
- [ ] Permit only these output-side drags: track source to Master input, track source directly to system sink, and Master output to system sink. Reject Master input/output misuse, MIDI-to-bus, system-to-bus, and bus-to-track paths.
- [ ] Emit the typed mixer-route intent for track-to-Master operations and the existing host-link intent for track/Master-to-system operations.
- [ ] Make confirmed user-managed mixer curves clickable for exact disconnect and include mixer routes in hover/error/hit-test state without conflating their keys with host links.
- [ ] Preserve filters and scopes: audio filtering hides Master; MIDI filtering does not fabricate bus endpoints; track scope shows the selected track plus Master; all-tracks scope shows all eligible sources.
- [ ] Adjust layout, scrolling, clipping, route painting, and visibility pruning for five columns and direct curves that bypass the bus column.
- [ ] Add egui tests for classification, facet compatibility, intent generation, confirmation/pending/error painting, exact disconnect, filters/scopes, direct bypass, empty host inventory, and large-graph layout.

Verification:

- [ ] Run focused `shoop_egui` connection-dialog tests.
- [ ] Manually verify drag, click-to-disconnect, hover, scrolling, audio/MIDI filters, all-track scope, and per-track scope in native and browser UI builds.

### Stage 6 — Persist Master and mixer routes transactionally

Depends on Stages 2–4. Complete before claiming the MVP acceptance criteria.

- [ ] Extend `BackendSessionData` and replacement mappings with buses, ordered channels, mixer routes, and bus-owned external ports.
- [ ] Use the existing session bus representation where it matches the architecture, and add explicit stable mixer-route documents rather than overloading host IDs or runtime indices. Bump the session document version.
- [ ] Define and validate the MVP capability shape: exactly one stereo Master bus, stable channel identities, audio-only routes from track output ports to Master channels, and no processors or editable controls.
- [ ] Migrate accepted legacy documents with no buses to a disconnected Master while preserving every direct track external connection. Reject malformed, duplicate, stale, incompatible, or unsupported bus graphs before backend mutation.
- [ ] Capture exact Master-to-system connections and track-to-Master routes; preserve intentional disconnection.
- [ ] Rebuild Master, tracks, routes, and external links in dependency order inside staged session replacement, and return complete source-to-runtime identity mappings.
- [ ] Preserve mixer state across compatible audio-driver switches and report unavailable system sinks through existing desired/confirmed connection semantics.
- [ ] Remove the current blanket rejection of non-empty bus documents only for the validated MVP shape.
- [ ] Update deterministic archive, validation, resampling, migration, session round-trip, browser transfer, and malformed-input tests.
- [ ] Update `docs/session_format_v1.md` and `docs/port_model.md` to describe the implemented Master shape, mixer links, five-column presentation, migration, and target parity.

Verification:

- [ ] Round-trip sessions containing no Master routes, several track-to-Master routes, fan-out, and Master external links.
- [ ] Verify new-session, legacy-load, failed-load rollback, same-rate replacement, resampled replacement, and driver-switch paths.

### Stage 7 — End-to-end validation and cleanup

Depends on all previous stages.

- [ ] Run an end-to-end native/dummy scenario: create two tracks, retain one direct system route, route both tracks to Master Left, route one track to Master Right, route Master channels to system sinks, verify summed/fan-out audio, disconnect each path, save, reload, and verify exact restoration.
- [ ] Run the equivalent Worker/AudioWorklet scenario and verify authoritative snapshots before and after every mutation.
- [ ] Verify a new session and a migrated legacy session expose a silent, externally disconnected Master and preserve existing direct track routes.
- [ ] Inspect a Perfetto capture if callback timing, graph installation, or route mutation behavior regresses; verify no callback graph build, lock, allocation, or unbounded route work.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace` in the environment selected by `.agents/info/build.md`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py` because Rust tests will change.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown` and run the documented Node/browser smoke suites where browser executables are available.
- [ ] Confirm documentation, protocol versions, session versions, fixtures, and release-facing behavior agree before completing the MVP.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
