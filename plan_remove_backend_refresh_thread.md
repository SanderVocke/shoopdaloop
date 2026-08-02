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

- [ ] No dedicated `QThread`, global update-thread singleton, or backup timer on a worker thread is used for frontend state refresh.
- [ ] Loop, composite-loop, loop-channel, port, and FX-chain frontend types no longer create paired backend QObjects or communicate through `backend_*` proxy signals.
- [ ] All frontend QObjects are accessed only on their Qt affinity thread; engine relationships use stable engine handles or identities rather than cross-thread QObject pointers.
- [ ] Existing public QML types, properties, signals, commands, session persistence, and test behavior remain compatible unless an internal backend-handle API is removed with all callers migrated.
- [ ] Realtime audio, MIDI, primitive loops, and composite schedules continue while the GUI thread is blocked; state converges to the current engine snapshot on the next GUI refresh.
- [ ] A control accepted before a GUI stall is not lost, and resuming the GUI does not replay an unbounded backlog of sampled state publications.
- [ ] Periodic GUI refresh performs no engine waits or bulk data transfer and emits property notifications only for observed changes.
- [ ] Realtime processing still allocates, frees, and locks no additional resources; preparation and reclamation remain off the realtime thread.
- [ ] Clean object destruction, session rebuilds, nested composites, ringbuffer adoption, port connections, and bulk loop data operations remain covered without stale-object crashes.
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

- [ ] Replace the worker-oriented update source with a GUI-affine refresh coordinator driven by `frameSwapped` plus a coalesced fallback `QTimer`.
- [ ] Provide an explicit refresh epoch/fence for deterministic tests instead of relying on repeated asynchronous update cycles.
- [ ] Collapse `BackendWrapper`'s split update methods and temporary update-data handoff into one GUI-thread refresh that publishes driver/session telemetry and a refresh-complete signal.
- [ ] Keep a temporary adapter for unmigrated backend QObjects so the tree remains functional during later stages.
- [ ] Test frame-driven, timer-driven, explicit, and coalesced refresh behavior; commit the stage.

### Stage 3 — Merge FX-chain and port object pairs

- [ ] Move FX-chain engine handles, deferred configuration, state sampling, and commands into the GUI object; remove its backend QObject and proxy signals.
- [ ] Move port engine handles, connection state, deferred configuration, telemetry, and commands into the GUI object.
- [ ] Replace FX-chain/port QObject dependency casts with GUI-thread handle extraction and stable engine references.
- [ ] Keep bulk data and connection work outside periodic refresh, and publish only changed properties.
- [ ] Remove the migrated backend bridges from module/build registration.
- [ ] Run FX-chain, port, autoconnect, MIDI-port, dry/wet, and lifecycle tests; commit each meaningful migration and the completed stage.

### Stage 4 — Merge primitive loop objects

- [ ] Move primitive loop creation, control methods, state cache, sync-source handling, and transition helpers into the GUI object.
- [ ] Replace backend-loop wrapper variants and multi-loop QObject casts with stable engine loop handles/identities.
- [ ] Publish `cycle_nr` from the authoritative engine counter and emit at most one current-cycle notification per refresh.
- [ ] Preserve immediate, delayed, aligned, grouped, clear, and ringbuffer-adoption behavior without refresh-order dependencies.
- [ ] Remove the primitive loop backend bridge and run loop, transition, sync, reorder, restoration, and stall tests; commit the stage.

### Stage 5 — Merge loop-channel objects

- [ ] Move audio/MIDI channel handles, state sampling, controls, and connection management into the GUI object.
- [ ] Resolve parent-loop and connected-port relationships through stable handles rather than backend QObject wrappers.
- [ ] Preserve asynchronous audio/MIDI data fetch and ensure load, clear, and ringbuffer operations do not become periodic-refresh work.
- [ ] Remove blocking queued invocations and the loop-channel backend bridge.
- [ ] Run channel, audio, MIDI, resampling, save/load, and data-fetch tests; commit the stage.

### Stage 6 — Merge composite-loop objects

- [ ] Move composite engine handles, schedule state, installation acknowledgements, controls, and mirror publication into the GUI object.
- [ ] Compile schedules from stable loop identities/handles; retain QObject references only to map active identities back to live GUI objects.
- [ ] Preserve nested schedule installation, pending options, schedule replacement, transition anticipation, and transactional ringbuffer adoption without polling-thread correctness dependencies.
- [ ] Ensure schedule preparation and result reclamation remain outside realtime processing and do not block periodic GUI refresh.
- [ ] Remove the composite backend bridge and run the complete composite, nested, restoration, ringbuffer, and GUI/file-I/O stall tests repeatedly; commit the stage.

### Stage 7 — Retire compatibility infrastructure

- [ ] Remove the update-thread QObject, `engine_update_thread` singleton, worker `QThread`, temporary adapters, crash-thread registration, and obsolete build/module entries.
- [ ] Remove remaining paired-object wrappers, backend proxy signals, cross-thread casts, and backend-only helper APIs.
- [ ] Connect QML-engine frame publication and the compatible refresh-interval setting directly to the GUI coordinator.
- [ ] Update test fences to request and observe one explicit GUI refresh epoch after engine commands settle.
- [ ] Add structural tests or checks proving migrated QObjects have GUI affinity and no removed backend/update types remain.
- [ ] Stress object creation/destruction and repeated session rebuilds, then commit the stage.

### Stage 8 — End-to-end validation

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build`.
- [ ] Run `cargo test --workspace --features shoop_engine/app_backend`, including realtime no-allocation tests.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 target/debug/shoopdaloop_dev.sh --self-test`.
- [ ] Repeatedly stress composite stalls, nested composites, session save/load, MIDI, port/channel publication, and object teardown.
- [ ] Audit every acceptance criterion against current code and test evidence.
- [ ] Push the branch, open or update the pull request, inspect failures with `gh`, and iterate until all required checks are green.
- [ ] Commit any final documentation/evidence updates and mark the plan complete.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
