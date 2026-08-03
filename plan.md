# Plan: application-wide Rust tracing, including `shoop_engine`

## Goals

1. Add coherent Tracy-visible instrumentation across the production Rust runtime: application startup/shutdown, common services, crash handling, CXX-Qt/frontend work, Lua and asynchronous helpers, control-thread engine APIs, driver lifecycle, and `shoop_engine` processing.
2. Make `shoop_engine` traces answer an end-to-end performance question: from a GUI/control request, through command queueing and graph scheduling, to the audio callback, session schedule, loops/channels/ports/composites/FX, state publication, and frontend refresh.
3. Preserve normal realtime correctness when tracing is disabled. When tracing is explicitly enabled for debugging, direct Tracy calls in audio/MIDI callbacks may allocate or lock internally and may perturb realtime performance; this exception must be confined to Tracy calls and clearly documented.
4. Keep tracing disabled by default and retain the existing `--tracing`, `--tracing-capture`, per-QML-file capture rotation, and default-off CI capture behavior.
5. Leave a maintainable coverage inventory so “the entire app Rust code” is auditable rather than inferred from a few representative traces.

## Immutable acceptance criteria

These criteria may not be weakened or changed without explicit user approval.

- A checked-in tracing coverage inventory accounts for every production `.rs` module under `src/rust`. Each module is marked as instrumented, indirectly covered by a named caller, or excluded with a concrete reason. Test-only code, generated/FFI declarations, proc-macro/build support, and trivial pure accessors may be excluded; runtime orchestration or processing code may not be silently omitted.
- The runtime crates `common`, `config`, `crashhandling`, `cxx_qt_lib_shoop`, `frontend`, `midi_processing`, `shoopdaloop`, and especially `shoop_engine` have useful coverage at their major operation and thread boundaries. The standalone `packaging` tool is also covered at command/subcommand and long-running scan/package stages. Build-only `macros` and `qt_header_bindings` are inventoried even when excluded.
- A captured trace demonstrates one causal path from frontend/control work through `app_backend`, engine command acceptance, an engine callback/cycle, session processing, state-mirror publication, and the following frontend refresh.
- Engine traces expose at least: driver/callback kind, callback duration and frame count, command queue depth and commands applied, graph rebuild scheduling/application, stale cycles, xruns, DSP load, capture under/overruns, processing-stage timing, loop/composite transitions, and state publication/drop counters.
- Detailed engine tracing covers the bounded processing categories—ports, channels, loops, composite runtime/timeline, and FX—without per-sample or per-MIDI-message zones and without unbounded-cardinality names.
- With tracing disabled, deterministic dummy/engine processing tests complete under `--rt-alloc-guard` with no allocation exception. Realtime Tracy helpers return before calling Tracy unless `--tracing`, `--tracing-capture`, or the subordinate `--tracing-engine-detail` option explicitly enables output.
- With tracing enabled, allocations and locks performed by direct Tracy calls are an accepted debugging-mode exception. Any Rust allocation made by the Tracy wrapper is enclosed in the narrowest practical `realtime_alloc_guard::allow_alloc` scope; unrelated engine processing remains guarded. Passing the Rust allocation guard is not presented as proof that Tracy's C++ internals are allocation-free or lock-free.
- Realtime callsites use direct `tracy-client` APIs rather than `tracing` subscriber spans/events or ordinary logging. Static source locations, bounded metadata, and no callstacks remain preferred to limit perturbation and trace volume, not as a realtime-safety guarantee.
- The same global enabled/output gate controls subscriber tracing, direct realtime Tracy zones, plots, frame marks, and capture quiescence. Rotating captures does not produce “zone ended twice,” instrumentation failures, truncated follow-on traces, or zones spanning a disconnect/reconnect.
- Tracing-disabled runs do not create captures and do not perform Tracy plot/span work beyond bounded atomic gate checks. Existing application and engine behavior and test results remain unchanged.
- Trace names and fields follow one documented naming scheme, use stable low-cardinality identifiers, and do not include audio payloads, raw MIDI payloads, session contents, secrets, or uncontrolled user strings.
- `cargo fmt --all`, a warning-free workspace build, the full Rust suite, frontend unit tests, targeted and broad QML tests, realtime allocation tests, capture rotation, and manual trace inspection all satisfy the final validation stage.
- Existing capture CLI, manifests, local documentation, and the default-off Linux CI trace archive continue to work. No automatic workflow trigger starts trace capture.

## Scope

### In scope

- Shared tracing enable/output state and helpers.
- Non-realtime `tracing` spans/events and Tracy plots.
- Gated direct Tracy zones/frame marks for carefully selected realtime boundaries, with an explicit debugging-mode exception for Tracy-internal allocation and locking.
- Existing engine profiler/stat/mirror mechanisms as the preferred way to move detailed realtime measurements to non-realtime consumers.
- Thread naming and spans around thread startup, queueing, waits, worker work, and shutdown.
- Current frontend architecture, including `FrontendRefresh` and GUI objects backed directly by engine handles/state mirrors.
- Engine control plane, drivers, callback cycle, schedule stages, graph scheduler, loops/channels/ports/composites/FX, and diagnostic publication.
- Coverage documentation, tests, and end-to-end trace review.

### Out of scope

- QML or C++ instrumentation except where a Rust FFI boundary is timed from Rust.
- Restoring deleted frontend backend/update-thread QObjects.
- Per-sample, per-audio-value, or unrestricted per-MIDI-event tracing.
- Logging or formatting on realtime threads.
- Capturing audio/MIDI/session payload contents.
- Making Tracy mandatory for normal runs or automatic CI events.
- Instrumenting every trivial getter, parser branch, generated bridge declaration, or test helper solely to increase a count.

## Design rules and constraints

### Two instrumentation paths

- **Non-realtime path:** use `tracing` spans/events so existing `SHOOP_LOG`/`RUST_LOG` behavior and `tracing_tracy::TracyLayer` remain the integration point. Use entered spans only for synchronous work; use explicit span lifetimes for queued/worker work.
- **Realtime path:** do not use the subscriber. Use a small direct `tracy-client` helper with static source locations, no call stacks, optional numeric values only, and a shared atomic gate. With tracing disabled, the helper must return before calling Tracy. With tracing enabled, Tracy-internal allocation and locking are accepted as debugging overhead.
- Prefer expanding `shoop_engine::profiling`, `Stats`, and preallocated trace snapshots when detailed data can be published atomically or through an existing bounded queue and plotted later on the control/GUI thread; use direct Tracy zones where exact callback-thread nesting is valuable.

### Shared gate and dependency direction

Introduce a minimal runtime tracing support crate (for example `shoop_tracing`) rather than making `shoop_engine` depend on the UI-oriented `common` crate. It should own:

- global tracing-enabled and tracing-output-enabled atomics;
- static realtime span/location helpers or the primitives needed by an engine-local helper;
- thread/frame helper functions that do not depend on logging or Qt.

`common` should use or re-export this gate for its subscriber filter, plotter, and capture quiescence. `shoop_engine` should depend only on this lightweight crate plus `tracy-client`. Preserve `prebuild` feature behavior and avoid dependency cycles.

### Realtime policy and tracing exception

- Normal realtime guarantees apply when no tracing CLI option is enabled. The disabled realtime helper path may perform bounded atomic gate checks but must not enter Tracy, allocate, lock, log, format, sleep, or wait.
- `--tracing` enables coarse direct Tracy zones in realtime callbacks. `--tracing-capture` implies the same behavior. `--tracing-engine-detail` is subordinate to tracing and enables additional per-node/category zones.
- Direct Tracy calls are the only approved exception to the usual realtime allocation and mutex rules. Keep `realtime_alloc_guard::allow_alloc` scopes inside the helper around Tracy begin/end/metadata operations; do not exempt surrounding engine work.
- Do not use the `tracing` subscriber, ordinary logging, application mutexes, sleeps, waits, or new blocking queues from realtime code. The exception does not authorize unrelated allocations or synchronization.
- Prefer static category locations, numeric metadata, no callstacks, and no `emit_text`. These reduce profiler overhead and cardinality even though tracing mode is not claimed realtime-safe.
- Prewarm Tracy/client state where practical, but do not claim this eliminates all allocation or locking: Tracy owns C++ thread-local producers and queue storage outside Rust's allocation guard.
- Document that tracing may cause xruns, deadline misses, or altered callback timing and that traces are debugging evidence rather than transparent performance measurements.
- Capture stop must disable both subscriber and direct realtime output, allow active zones to drain, then disconnect. Capture start re-enables output only after the new capturer connects.

### Naming and cardinality

Use stable snake-case names grouped by layer, for example:

- `app.*`, `frontend.*`, `worker.*`
- `engine.control.*`, `engine.graph.*`, `engine.driver.*`
- `engine.rt.callback`, `engine.rt.commands`, `engine.rt.session`
- `engine.rt.ports`, `engine.rt.channels`, `engine.rt.loops`, `engine.rt.composites`, `engine.rt.fx`

Use stable plot prefixes per session/object type. Dynamic object labels may be used only off the realtime thread and must be sanitized/bounded. Record numeric values such as frame count, queue depth, sequence number, stage kind, object index, mode, and drop count without embedding them in span names.

### Coverage granularity

Instrument operation boundaries and state transitions, not every function. A module is indirectly covered only when a named enclosing span genuinely includes its meaningful work. Pure algorithms should receive spans at their public orchestration boundary; hot inner loops should remain free of nested tracing unless a bounded detail mode is justified and tested.

## Staged implementation

Stages are ordered. A later stage may not begin until the preceding stage's verification is recorded, except for independent inventory/documentation work.

### Stage 0 — Baseline and coverage inventory

- [x] Create a checked-in Rust tracing coverage inventory listing every production module by crate, thread context, major operations, realtime status, intended spans/plots, and any exclusion rationale.
- [x] Remove stale inventory entries for deleted backend/update-thread QObjects and map their former diagnostics to current GUI/state-mirror/frontend-refresh paths.
- [x] Record baseline traces for a targeted QML test and a controlled dummy-engine run, including current zones, trace size, instrumentation warnings, callback timing, and capture rotation behavior.
- [x] Record baseline tracing-disabled and tracing-enabled engine processing measurements using deterministic tests; do not set a fragile wall-clock threshold. (The captured baseline proves engine zones are absent, so existing profiler/allocation results are the engine baseline.)
- [x] Identify all thread creation/callback entrypoints and all regions protected by `realtime_alloc_guard`.

Verification:

- [x] Inventory has no unclassified production Rust module.
- [x] `cargo build` and targeted baseline tests pass before behavior changes.
- [x] Baseline `.tracy` files parse with the matching Tracy tools and are retained for before/after comparison.

Commit this stage as the inventory/baseline milestone.

### Stage 1 — Shared tracing infrastructure and gate proof

Depends on Stage 0.

- [x] Add the minimal shared tracing support crate and move the enabled/output gate behind it while preserving current public behavior through `common`.
- [x] Add a separate `--tracing-engine-detail` gate and CLI option that requires `--tracing` or `--tracing-capture`.
- [x] Add realtime location/span helpers with a fast disabled path, narrow allocation-permitted scopes around direct Tracy operations, and a best-effort prewarm API. (Stage 5 connects actual engine locations to driver activation.)
- [x] Make capture quiescence disable subscriber spans, direct realtime zones, frame marks, and plots through the same output gate.
- [x] Add unit tests for gate combinations, disabled behavior, prewarming/cached locations, capture disable/re-enable, and static-location reuse.
- [x] Add allocation-guard tests proving the disabled path does not allocate and that enabled tracing does not require exempting surrounding engine processing. Document that these tests cannot observe or certify Tracy's C++ allocator/locking behavior.

Verification:

- [x] Existing live tracing and per-QML capture still work.
- [x] Two rotated captures both parse and contain valid follow-on zones with no Tracy instrumentation warning.
- [x] Realtime helper tests pass with the allocation guard enabled: no exception when tracing is disabled, and only the direct Tracy wrapper is allocation-permitted when tracing is enabled.
- [x] Warning-free workspace build passes on the host platform; platform-specific code remains `cfg`-clean.

Commit this stage before adding broad instrumentation.

### Stage 2 — Application, support crates, and worker lifecycle

Depends on Stage 1; contains no realtime instrumentation.

- [ ] Instrument `shoopdaloop` bootstrap, configuration loading, argument-driven exits, crash-handler setup, application creation/event loop, self-test setup, and normal shutdown.
- [ ] Instrument major `common` environment/filesystem/shell operations without tracing the logger or capture handler recursively.
- [ ] Instrument `config` loading/path resolution and `crashhandling` client/server/process/message lifecycle, excluding signal/exception handlers and `atexit` paths that cannot safely use tracing TLS.
- [ ] Instrument meaningful CXX-Qt helper crossings in `cxx_qt_lib_shoop`; inventory trivial conversion/accessor helpers as indirectly covered or excluded.
- [ ] Instrument `midi_processing` batch conversions with counts only, never payload bytes.
- [ ] Add top-level spans to `packaging` subcommands and long-running dependency scan/copy/archive stages; keep build/proc-macro crates inventoried as build-only.
- [ ] Name application-owned worker threads at startup and trace queue/wait/work/shutdown lifecycles.

Verification:

- [ ] Focused unit tests for each touched crate pass.
- [ ] A startup/quit trace shows ordered app, config, crash-handler, Qt, and shutdown spans.
- [ ] Packaging smoke commands retain exit behavior and produce useful top-level spans when tracing is explicitly enabled.

Commit this stage as a non-realtime runtime milestone.

### Stage 3 — Current frontend and GUI/state propagation

Depends on Stage 2 and the current post-refresh-thread architecture.

- [ ] Instrument `FrontendRefresh` request coalescing, queued delay, fallback timer source, refresh duration, and a named frontend-refresh frame mark.
- [ ] Instrument `BackendWrapper::refresh` and plot current engine/driver stats: readiness, backend kind, cycles/frames, pending/applied commands, xruns, stale cycles, DSP load, buffer state, and capture under/overruns.
- [ ] Add state-change plots/spans to current GUI objects for loops, composite loops, loop channels, ports, FX chains, MIDI control, and connection cache publication; do not restore deleted backend QObjects.
- [ ] Instrument frontend-to-`app_backend` control calls with operation category, bounded object/session IDs, queue outcome, and synchronous wait duration.
- [ ] Instrument QML load/unload/reload, Lua evaluation/callback boundaries, file/session I/O, schema/settings work, waveform/MIDI rendering, async tasks, click-track work, and session-control dispatch at their coarse operation boundaries.
- [ ] Trace worker handoff latency and completion for frontend-owned threads without holding spans across unrelated work.
- [ ] Remove or repurpose stale plotter fields that no longer have a current producer after the backend-refresh-thread removal.

Verification:

- [ ] Frontend unit tests pass warning-free.
- [ ] Targeted QML traces show control request, state propagation, and subsequent frontend refresh with stable object labels.
- [ ] Tracing-disabled targeted QML output/results remain unchanged and no capture process starts.

Commit this stage as the frontend coverage milestone.

### Stage 4 — `shoop_engine` control plane and background workers

Depends on Stages 1 and 3; keep this stage off realtime callbacks.

- [ ] Instrument `app_backend` session/driver lifecycle, object pending→ready/failed/closed transitions, control validation, queue outcomes, graph dirtying, state reads, connection-cache refresh, and reclamation workers.
- [ ] Instrument `EngineHandle` reservation/send/send-and-wait/result/reclaim/poll operations with command sequence, pending depth, wait duration, timeout/full/disconnected outcome, and trace-snapshot drops.
- [ ] Instrument graph topology construction, schedule compilation, `GraphScheduler` arm/coalescing/apply/flush, and stale-to-current transitions.
- [ ] Instrument composite plan compilation/validation/installation and non-realtime control requests without logging target tables or user data.
- [ ] Instrument LV2/Carla discovery, instantiation, UI/state operations, and non-realtime setup/teardown separately from plugin processing.
- [ ] Name engine-owned non-realtime workers and expose queue depth, coalescing, wait, and dropped-work plots.

Verification:

- [ ] Engine control, graph, composite, external-port, JACK/CPAL mock, and app-backend tests pass.
- [ ] A control-heavy test trace correlates command sequence numbers from frontend enqueue through engine acknowledgement/reclamation.
- [ ] Queue-full, timeout, failed object creation, and graph-rebuild paths produce bounded diagnostic events without changing returned errors.

Commit this stage before touching realtime processing.

### Stage 5 — `shoop_engine` realtime callbacks and processing

Depends on Stage 1's gate and exception-scope proof and Stage 4's control-plane coverage.

- [ ] Prewarm realtime source locations and Tracy client state where practical before JACK/CPAL/dummy activation; treat this as overhead reduction rather than proof of allocation-free behavior.
- [ ] Add coarse static zones around JACK, CPAL output/input coordination, and dummy callback cycles; attach frame count and driver kind as numeric values.
- [ ] Instrument `Engine::process`/`run_cycle` into bounded zones for command draining, graph-staleness publication, session cycle, diagnostic publication, and cycle completion.
- [ ] Expand engine stats/profiling for callback budget, command count, pending depth, schedule generation, stale/stuck cycles, and trace snapshot drops.
- [ ] Instrument `Session::process` by schedule category: port prepare/process, channel prepare/process/finalize, loop group, composite timeline/runtime, external routing, and FX processing.
- [ ] Add optional engine-detail zones for individual scheduled nodes using static category locations and numeric indices; keep it disabled unless explicitly requested.
- [ ] Expose loop mode/position/transition, composite mode/iteration/fault, port peaks/event counts, channel mode/record state, and FX active/bypass metrics through existing mirrors/stats or preallocated snapshots, then emit plots on a non-realtime consumer.
- [ ] Cover MIDI driver staging and audio capture-ring under/overrun counters without tracing individual events or samples.
- [ ] Verify every direct realtime callsite is subordinate to both global tracing gates and uses only the narrow helper exception. Verify engine work surrounding Tracy remains subject to the normal realtime guard; do not claim Tracy itself is bounded, lock-free, or allocation-free.

Verification:

- [ ] Run engine processing, dummy driver, CPAL mock, composite runtime/timeline, channel, port, MIDI, and FX tests under `--rt-alloc-guard` equivalent setup with tracing disabled; repeat representative scenarios with coarse and engine-detail tracing to verify the scoped Tracy exception.
- [ ] Tracing-disabled tests have no allocation exception. Tracing-enabled tests have no allocation exception outside the explicit Tracy helper scope, no application deadlock or logging call, and no Tracy instrumentation warning. Record any xruns or timing perturbation as expected profiling overhead rather than hiding it.
- [ ] A controlled trace shows callback→engine→session stage nesting and non-realtime plots agree with engine stats/profiling reports.
- [ ] Deterministic dummy runs show no new stuck/stale cycles or processing-result differences.

Commit coarse realtime coverage first; commit optional detailed coverage separately after its overhead and trace volume are reviewed.

### Stage 6 — Coverage closure and consistency audit

Depends on Stages 2–5.

- [ ] Revisit every inventory row and inspect actual trace evidence rather than accepting source annotations alone.
- [ ] Instrument uncovered runtime orchestration or document a valid indirect-coverage/exclusion reason.
- [ ] Audit span lifetime correctness across callbacks, queued work, threads, early returns, errors, and capture rotation.
- [ ] Audit naming/cardinality and remove duplicate, noisy, per-item, payload-bearing, or misleading instrumentation.
- [ ] Audit all realtime modules for accidental `tracing` subscriber use, logging, unrelated allocation/locking, overly broad allocation-permitted scopes, dynamic high-cardinality locations, and callstack use.
- [ ] Update developer documentation with tracing levels, engine detail option, naming, expected overhead/safety rules, and trace interpretation examples.

Verification:

- [ ] Inventory checker/script reports zero unclassified production modules.
- [ ] Representative traces contain every acceptance-criteria layer and no uncontrolled-cardinality names.
- [ ] Disabled, coarse, and detailed modes are distinguishable and documented.

Commit the completed inventory and documentation as a milestone.

### Stage 7 — Final end-to-end validation

Depends on all prior stages. Do not mark the plan complete based only on source inspection or passing unit tests.

- [ ] Run `cargo fmt --all` and `git diff --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build`.
- [ ] Run `cargo test --workspace --features shoop_engine/app_backend`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo test -p frontend --lib`.
- [ ] Run targeted QML tests while iterating, then the broad QML suite with tracing disabled.
- [ ] Run a deterministic dummy QML/engine scenario under `--rt-alloc-guard` with tracing disabled. Repeat with `--tracing` and engine-detail tracing to verify that only direct Tracy helper operations use the documented exception; record profiling-induced timing changes.
- [ ] Run at least two QML files with `--tracing-capture --rt-alloc-guard`; verify one non-empty trace and one successful manifest row per file, no allocation exception outside Tracy helper scopes, no orphan capture process, and no capture instrumentation failure.
- [ ] Parse all generated traces with the matching `tracy-csvexport` and open representative captures in Tracy.
- [ ] Confirm the trace contains: application lifecycle, frontend control span, engine command sequence, graph scheduling where applicable, realtime callback/cycle, session processing categories, state publication, frontend refresh, and engine health plots.
- [ ] Compare trace values against engine `Stats`, profiler reports, QML outcomes, and capture manifests; do not rely on visual plausibility alone.
- [ ] Run available JACK/CPAL/LV2 integration tests and record environmental skips separately from regressions.
- [ ] Confirm automatic workflow events still leave capture disabled and statically validate the existing opt-in trace archive path. Do not claim a manual CI capture run unless one is explicitly requested and actually performed.
- [ ] Update PR documentation with exact commands, pass counts, trace evidence, known environment limitations, and any justified inventory exclusions.

If Tracy 0.13.1 tools are unavailable, if the tracing-disabled realtime path cannot be validated under the allocation guard, if a representative trace cannot be parsed/opened, or if any production module remains unclassified, stop with the gathered evidence and blocker; do not mark the work complete.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
