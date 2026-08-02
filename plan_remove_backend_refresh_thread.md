# Remove the frontend backend-refresh thread

Status: in progress

Branch: `remove_backend_update_thread`

Base: `origin/master` at `937997b5`

## Goals

- Remove the dedicated backend state-refresh thread and the paired GUI/backend QObject architecture.
- Keep engine processing, including composite scheduling, independent of GUI stalls.
- Make GUI-thread objects consume current engine mirrors directly and enqueue controls without cross-thread QObject proxies.
- Reduce refresh backlogs, lifecycle races, and stale QObject access while preserving user-visible behavior.

## Scope

This includes the refresh coordinator, backend wrapper publication, loop, composite-loop, loop-channel, port, and FX-chain frontend objects; the engine mirror/API prerequisites they need; QML integration; and tests. It does not redesign realtime audio processing, session semantics, or the visible QML feature set.

## Acceptance criteria (immutable)

- [x] No dedicated `QThread`, global update-thread singleton, or backup timer on a worker thread is used for frontend state refresh.
- [x] Loop, composite-loop, loop-channel, port, and FX-chain frontend types no longer create paired backend QObjects or communicate through `backend_*` proxy signals.
- [x] All frontend QObjects are accessed only on their Qt affinity thread; engine relationships use stable engine handles or identities rather than cross-thread QObject pointers.
- [x] Existing public QML types, properties, signals, commands, session persistence, and test behavior remain compatible unless an internal backend-handle API is removed with all callers migrated.
- [x] Realtime audio, MIDI, primitive loops, and composite schedules continue while the GUI thread is blocked; state converges to the current engine snapshot on the next GUI refresh.
- [x] A control accepted before a GUI stall is not lost, and resuming the GUI does not replay an unbounded backlog of sampled state publications.
- [x] Periodic GUI refresh performs no engine waits or bulk data transfer and emits property notifications only for observed changes.
- [x] Realtime processing still allocates, frees, and locks no additional resources; preparation and reclamation remain off the realtime thread.
- [x] Clean object destruction, session rebuilds, nested composites, ringbuffer adoption, port connections, and bulk loop data operations remain covered without stale-object crashes.
- [ ] Formatting, warning-free build, Rust tests, the full QML suite, relevant stress tests, and all required pull-request checks pass.

## Design rules

- The engine and its state mirrors are authoritative; frontend cached state is presentation state only.
- One GUI-affine refresh coordinator may coalesce `frameSwapped` and fallback-timer requests. It must not own or start a worker thread.
- GUI stalls may skip intermediate samples. Refresh resumes from the latest snapshot rather than replaying stale samples.
- Controls enqueue engine work immediately and must not depend on periodic refresh ordering for correctness.
- Initialization and dependency resolution are event- or setter-driven. Refresh may retry pending work but must not be its only trigger.
- Stable engine handles and identities represent loop, channel, port, FX-chain, and composite relationships. QObject references are retained only for GUI presentation and are never dereferenced off the GUI thread.
- Potentially expensive compilation, copying, I/O, and reclamation use explicit asynchronous jobs when needed; they are not hidden inside periodic refresh.
- Preserve compatibility for the existing refresh-interval setting while giving it GUI-refresh semantics.
- Keep each migration stage buildable and testable; use temporary adapters only while unmigrated object pairs remain.

## Implementation plan

### Stage 1 — Lock down behavior and engine prerequisites

- [x] Add focused coverage for a control queued immediately before a GUI stall, continued primitive/composite execution during the stall, and convergence after one resumed refresh.
- [x] Add an authoritative primitive-loop cycle/generation counter to the engine mirror so cycle signals do not depend on observing position wraparound at 40 Hz.
- [x] Add mirror tests and realtime no-allocation coverage for the new publication field.
- [x] Inventory periodic refresh methods and move any wait, bulk copy, or blocking query behind an explicit command or asynchronous task before migrating its object.
- [x] Verify targeted engine and frontend tests, then commit the stage.

### Stage 2 — Introduce GUI-affine refresh publication

- [x] Replace the worker-oriented update source with a GUI-affine refresh coordinator driven by `frameSwapped` plus a coalesced fallback `QTimer`.
- [x] Provide an explicit refresh epoch/fence for deterministic tests instead of relying on repeated asynchronous update cycles.
- [x] Collapse `BackendWrapper`'s split update methods and temporary update-data handoff into one GUI-thread refresh that publishes driver/session telemetry and a refresh-complete signal.
- [x] No temporary adapter was needed: all paired objects were migrated in the same buildable milestone.
- [x] Test frame-driven, timer-driven, explicit, and coalesced refresh behavior; commit the stage.

### Stage 3 — Merge FX-chain and port object pairs

- [x] Move FX-chain engine handles, deferred configuration, state sampling, and commands into the GUI object; remove its backend QObject and proxy signals.
- [x] Move port engine handles, connection state, deferred configuration, telemetry, and commands into the GUI object.
- [x] Replace FX-chain/port QObject dependency casts with GUI-thread handle extraction and stable engine references.
- [x] Keep bulk data and connection work outside periodic refresh, and publish only changed properties.
- [x] Remove the migrated backend bridges from module/build registration.
- [x] Run FX-chain, port, autoconnect, MIDI-port, dry/wet, and lifecycle tests; commit each meaningful migration and the completed stage.

### Stage 4 — Merge primitive loop objects

- [x] Move primitive loop creation, control methods, state cache, sync-source handling, and transition helpers into the GUI object.
- [x] Replace backend-loop wrapper variants and multi-loop QObject casts with stable engine loop handles/identities.
- [x] Publish `cycle_nr` from the authoritative engine counter and emit at most one current-cycle notification per refresh.
- [x] Preserve immediate, delayed, aligned, grouped, clear, and ringbuffer-adoption behavior without refresh-order dependencies.
- [x] Remove the primitive loop backend bridge and run loop, transition, sync, reorder, restoration, and stall tests; commit the stage.

### Stage 5 — Merge loop-channel objects

- [x] Move audio/MIDI channel handles, state sampling, controls, and connection management into the GUI object.
- [x] Resolve parent-loop and connected-port relationships through stable handles rather than backend QObject wrappers.
- [x] Preserve asynchronous audio/MIDI data fetch and ensure load, clear, and ringbuffer operations do not become periodic-refresh work.
- [x] Remove blocking queued invocations and the loop-channel backend bridge.
- [x] Run channel, audio, MIDI, resampling, save/load, and data-fetch tests; commit the stage.

### Stage 6 — Merge composite-loop objects

- [x] Move composite engine handles, schedule state, installation acknowledgements, controls, and mirror publication into the GUI object.
- [x] Compile schedules from stable loop identities/handles; retain QObject references only to map active identities back to live GUI objects.
- [x] Preserve nested schedule installation, pending options, schedule replacement, transition anticipation, and transactional ringbuffer adoption without polling-thread correctness dependencies.
- [x] Ensure schedule preparation and result reclamation remain outside realtime processing and do not block periodic GUI refresh.
- [x] Remove the composite backend bridge and run the complete composite, nested, restoration, ringbuffer, and GUI/file-I/O stall tests repeatedly; commit the stage.

### Stage 7 — Retire compatibility infrastructure

- [x] Remove the update-thread QObject, `engine_update_thread` singleton, worker `QThread`, temporary adapters, crash-thread registration, and obsolete build/module entries.
- [x] Remove remaining paired-object wrappers, backend proxy signals, cross-thread casts, and backend-only helper APIs.
- [x] Connect QML-engine frame publication and the compatible refresh-interval setting directly to the GUI coordinator.
- [x] Update test fences to request and observe explicit GUI refresh epochs after engine commands settle.
- [x] Add structural tests or checks proving migrated QObjects have GUI affinity and no removed backend/update types remain.
- [x] Stress object creation/destruction and repeated session rebuilds, then commit the stage.

### Implementation evidence through Stages 2–7

- `FrontendRefresh` is a GUI-thread-owned singleton with a parented `QTimer`; `frameSwapped()` and timer requests coalesce through one queued refresh.
- `BackendWrapper::refresh()` samples driver/session telemetry synchronously on the GUI thread, increments `refresh_epoch`, and emits the compatibility completion signal. `update_interval_ms` now configures the GUI fallback timer.
- The five promoted GUI types directly own cloneable `app_backend` handles. QObject dependencies are inspected only from direct GUI-thread calls and guarded with `QPointer` where their lifetime is not structural.
- Audio/MIDI file jobs extract cloneable engine channel handles before spawning; periodic updates only poll mirrors and never wait or transfer bulk data.
- Static checks find no removed backend/update modules or types, no blocking queued connections/waits in promoted periodic updates, and no thread creation in the refresh coordinator.
- Warning-free builds pass. The 192-case QML suite passes with 191 passed and the expected unsupported CPAL case skipped; focused backend refresh, Lua controls, composite, and dry/wet tests also pass.
- Stress evidence: 10 complete composite-running runs, 5 session save/load runs, 10 MIDI runs, and 5 port/channel dry/wet runs passed.
- Final local gates: formatting and diff checks pass; warning-free build passes; the serialized workspace suite passes with missing hardware backends explicitly allowed (including all 19 no-allocation tests); final QML suite passes 193/194 with the one supported CPAL skip. The ordinary parallel workspace run exposed a pre-existing Carla/JUCE test-process teardown SIGSEGV, while its 582-test engine library body passed and the serialized required suite completed cleanly.
- Instrumented CI exposed a port/channel initialization-order race hidden by ordinary timing. Channel refresh now retries stable-handle port relationship resolution, while setter and queued initialization notifications remain the primary triggers.
- macOS ARM exposed controls issued after composite installation was queued but before its lightweight creation command was acknowledged. Pending composite handles now accept such controls in the shared sequenced engine queue, with a deterministic app-backend regression test; the complete composite QML suite, including countdown and GUI-stall cases, passes locally.

### Acceptance audit evidence

1. Static searches show no update-thread modules/types, worker `QThread`, move-to-thread call, or refresh-thread crash registration; `FrontendRefresh` owns only a GUI-affine `QTimer`.
2. The five `*_backend` QObject implementations and bridges are deleted and absent from build/module registration. Promoted GUI objects own the engine handles.
3. All remaining QObject casts/invocations are direct GUI-thread paths. Async file/data work receives cloned `AnyBackendChannel` handles before spawning and captures no GUI QObject.
4. The unchanged QML components load and the complete behavior, Lua-control, persistence, restoration, audio, MIDI, FX, and port suite passes.
5. `test_ui_frozen` and `test_fileio_frozen` prove realtime primitive/composite progress through GUI stalls and current-state convergence after refresh.
6. The stall test queues the control before blocking and checks its resulting cycle/mode. `FrontendRefresh::refresh_queued` coalesces requests, so a stall can leave at most one queued publication.
7. `BackendWrapper::refresh` and each promoted periodic `update` poll mirrors without command fences, waits, blocking invocations, file I/O, or bulk channel copies; field notifications compare previous/current values.
8. All 19 realtime no-allocation tests pass, including composite timelines, dense events, ringbuffer adoption, channels, MIDI, and command application.
9. Full and repeated composite, restoration, MIDI, port/channel, data, and teardown runs pass without stale QObject crashes.
10. Local gates are green. Cross-platform pull-request checks remain the final open acceptance item.

### Stage 8 — End-to-end validation

- [x] Run `cargo fmt --all -- --check` and `git diff --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build`.
- [x] Run `cargo test --workspace --features shoop_engine/app_backend`, including realtime no-allocation tests.
- [x] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 target/debug/shoopdaloop_dev.sh --self-test`.
- [x] Repeatedly stress composite stalls, nested composites, session save/load, MIDI, port/channel publication, and object teardown.
- [x] Audit every acceptance criterion against current code and test evidence.
- [ ] Push the branch, open or update the pull request, inspect failures with `gh`, and iterate until all required checks are green.
- [ ] Commit any final documentation/evidence updates and mark the plan complete.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
