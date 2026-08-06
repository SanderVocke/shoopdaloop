# Milestone 3 Plan: Browser Web Audio Input/Output Driver

## Status

Complete. Hosted secure browser runs now use direct `web-sys` Web Audio and a dedicated worklet-owned Shoop engine after an explicit microphone enable action. Native runs retain the threaded dummy backend, and direct-file artifacts expose only explicit offline dummy operation.

`BROWSER_AUDIO_CONTRACT.md` records protocol, storage, routing, lifecycle, artifact, and real-time limits. `EGUI_FEATURE_PARITY_MATRIX.md` records detailed implementation evidence. Chrome 147 and Firefox 150 deterministic fake-media workflows prove non-zero capture, recording, waveform, playback, and output. Physical hardware was unavailable on the validation host, and Safari remains an explicit untested compatibility limitation.

Milestone 2 remains the completed historical record for the unified dummy-engine application.

## Goals and scope

Make the browser build of `shoopdaloop_egui` request access to the default microphone, run the Shoop engine from an `AudioWorklet`, and send engine output to the browser's default audio destination. Browser selection of this driver is automatic; permission and autoplay policy are satisfied through an explicit user gesture.

The milestone includes:

- A browser audio controller built directly on `web-sys` for `AudioContext`, `MediaDevices::getUserMedia`, `MediaStreamAudioSourceNode`, `AudioWorklet`, `AudioWorkletNode`, lifecycle events, and errors.
- A minimal JavaScript `AudioWorkletProcessor` registration/copy shim, with driver policy, engine state, and DSP remaining in Rust.
- A dedicated Wasm worklet module that privately owns `shoop_engine` state and processes each Web Audio render quantum.
- One `AudioContext` graph from microphone to worklet to destination, using the context's actual sample rate so browser/device rate conversion remains the browser's responsibility.
- Asynchronous, bounded command/result and snapshot transport between the browser application and the worklet-owned engine.
- Default audio routing sufficient for mono and stereo direct tracks without waiting for the connections-dialog milestone.
- Observable permission, startup, running, suspended, denied, unavailable, and failed states with retry behavior.
- Hosted browser artifacts and CI coverage for granted and denied microphone permission, real worklet callbacks, recording, monitoring, playback, and lifecycle recovery.
- Preservation of the native egui dummy runtime and all retained native JACK, CPAL, Midir, LV2, Qt/frontend, and engine test paths.

Out of scope:

- Web MIDI or any browser MIDI-device integration.
- Browser CPAL, Firewheel, ScriptProcessorNode, or another audio abstraction layered over Web Audio.
- Microphone or output device selection beyond browser defaults, hot switching by device ID, and the full settings/connections UI.
- Echo cancellation, noise suppression, or automatic gain control as Shoop DSP features. The driver requests raw capture where supported and reports browser negotiation rather than emulating unsupported constraints.
- A fixed internal sample rate. The browser engine runs at the actual `AudioContext` rate.
- Session persistence, media import/export, sample-rate migration for saved material, and cross-run audio compatibility.
- Web MIDI, FX/plugin hosting, dry/wet topology, scripting, composite loops, or other deferred parity areas.
- Claiming microphone support from a directly opened `file:` artifact. Physical audio requires a secure browser context; offline/dummy behavior remains explicit.
- Replacing the native egui dummy driver with a native physical driver or changing the retained Qt production entry point.

## Immutable acceptance criteria

These criteria may not change without explicit user approval.

1. A supported secure browser run of `shoopdaloop_egui` automatically selects the new Web Audio backend; it never selects CPAL or silently continues elapsed-time dummy processing after physical audio starts. Native builds continue selecting their existing threaded dummy backend.
2. Browser startup presents an explicit enable-audio action. Invoking it synchronously begins the user-gesture-sensitive startup path, requests the default microphone through `getUserMedia`, and reaches a running state without requiring a driver picker. Denial, dismissal, missing secure context, missing API support, and retry are visible and non-fatal.
3. The physical signal graph is one `AudioContext`: default microphone `MediaStreamAudioSourceNode` to `AudioWorkletNode` to default destination. The worklet processes the actual context sample rate and render-quantum length; Shoop does not perform redundant device-rate conversion.
4. A dedicated worklet Wasm module exclusively owns the browser engine/session and is the only source of engine audio-clock advancement while Web Audio is active. The UI animation clock never calls dummy elapsed-time advancement on that session.
5. Default routing makes current mono and stereo direct tracks usable without a connections UI: capture channels feed track inputs deterministically, track outputs mix to the destination deterministically, mono/stereo mismatches have documented behavior, and input monitoring remains under the existing track control to reduce accidental feedback.
6. With an automated non-silent fake microphone, the browser application can create a direct audio track, enable monitoring, record non-zero microphone samples into a loop, stop, publish non-empty/non-zero waveform data, play the recording to non-zero worklet output, and continue updating authoritative engine state.
7. Browser control is asynchronous and bounded. No browser main-thread operation waits for a worklet result, and no worklet operation waits for the UI. Commands, acknowledgements, state snapshots, and waveform transfers have explicit capacities, ordering, backpressure, stale-ID behavior, and observable overflow/failure semantics.
8. The AudioWorklet process path contains no blocking mutex, sleep, condition-variable wait, thread join, filesystem/DOM call, unbounded queue drain, or per-quantum `postMessage`. Steady-state input copy, engine processing, output copy, and state publication are allocation-free after initialization and satisfy the engine's real-time guards.
9. Worklet-side topology mutation, graph scheduling, recording storage, object lifetime, and waveform extraction use bounded prepared/reusable storage. Capacity exhaustion fails visibly or defers work without allocating in the render callback, corrupting state, silently dropping intent, or allowing unbounded callback work.
10. Callback-budget, queue-overflow, pool-exhaustion, worklet-error, capture/output activity, context state, actual sample rate, render quantum, and xrun/overrun diagnostics are observable through backend/application status and browser diagnostics at a bounded display cadence.
11. Browser suspension, tab throttling, permission loss, media-track end, worklet failure, retry, and shutdown/drop do not deadlock, leak a live microphone track, run duplicate engines, or trigger elapsed-time catch-up. Resumption continues from audio callback time and records an observable discontinuity where appropriate.
12. The browser dependency and artifact graph uses `web-sys`/`wasm-bindgen` plus repository-owned glue and excludes browser CPAL, Firewheel, Midir, JACK, LV2, frontend, Qt, X11, and Wayland. `shoop_egui` remains backend-free and unaware of Web Audio.
13. The worklet module, its JavaScript shim, and all required browser assets are produced reproducibly by the package build and CI. Hosted HTTPS/localhost artifacts support physical audio; the self-contained artifact remains loadable and either runs an explicitly selected dummy/offline mode or presents a precise secure-context limitation without claiming microphone support.
14. Existing native behavior and verification remain intact: `shoopdaloop_egui` starts with its threaded dummy backend, retained native engine drivers keep their feature/build/test coverage, and no AudioWorklet/Web Audio dependency enters native runtime paths.
15. Automated tests cover protocol ordering/capacity, 128-frame full-duplex engine cycles, graph/control changes during processing, sustained recording without render-path allocation, permission outcomes, callback progress, non-zero input/output, suspension/recovery, cleanup, dependency isolation, and native regressions. Browser tests use a deterministic fake microphone rather than treating silence or status text as proof of audio I/O.
16. `EGUI_REPLACEMENT_PROJECT.md`, `EGUI_FEATURE_PARITY_MATRIX.md`, root/package run documentation, browser UI text, artifact descriptions, and CI names accurately distinguish native dummy operation, hosted browser physical audio, permission requirements, secure-context requirements, offline fallback, and the absence of Web MIDI.

## Design rules and important constraints

- Keep the dependency direction in `EGUI_REPLACEMENT_PROJECT.md`. `shoop_egui` renders driver state from plain API values and emits intents; it never owns `AudioContext`, `MediaStream`, `AudioWorkletNode`, engine handles, or permission promises.
- Use Web Audio directly. A small JavaScript `AudioWorkletProcessor` shim is allowed only where the browser requires JavaScript registration and planar-buffer access; lifecycle policy, command semantics, DSP, and authoritative state remain Rust-owned.
- Prefer a private worklet Wasm memory and `MessagePort` control protocol over sharing the UI Wasm instance. Audio samples never cross `MessagePort`; only bounded control, status, meter, and requested content data do. This avoids UI/worklet mutexes and cross-origin-isolated shared memory unless implementation evidence proves a documented revision necessary.
- Compile the worklet as a dedicated artifact with a narrow numeric/typed ABI. Do not load eframe, `shoop_app`, DOM APIs, native drivers, or the browser UI into the worklet module.
- The worklet exclusively owns mutable `Session`/engine state. Never expose it through `Arc<Mutex<Session>>` or process audio on the UI thread.
- Refactor backend operations into asynchronous submission plus completion/state events where the browser cannot return a worklet result synchronously. Native adapters may complete immediately, but application semantics and stable IDs remain shared.
- Use application-assigned stable IDs in commands so browser creation never blocks waiting for an engine-generated ID. Pending, acknowledged, failed, and stale operations must reconcile deterministically.
- Keep the JavaScript process callback allocation-free: instantiate Wasm, allocate channel views, and bind exports before playback. Reuse views unless Wasm memory growth invalidates them; memory growth during processing is prohibited and diagnosed.
- Run the engine at `AudioContext.sampleRate`. Use browser conversion at microphone and destination boundaries. If future persistence requires a fixed/content rate, add that conversion in the media/persistence layer rather than the device callback.
- Assume the common 128-frame Web Audio quantum but always consume the callback-provided length and fail clearly if an engine limit is exceeded. Prewarm buffers for every supported quantum/channel shape before the context enters `running`.
- Replace lock-backed external capture plumbing with worklet-local borrowed/staged cycle buffers. Do not reuse native CPAL capture rings, external-connection mutexes, worker waits, or the elapsed-time dummy clock.
- Bound commands processed per render quantum and coalesce superseded controls such as gain, balance, and meters. Structural commands must not starve transport commands or exceed a callback budget without deferral.
- Prepare or reserve topology, schedule, channel, port, and recording storage before it is needed by `process()`. A render callback must never extend a `Vec`, grow Wasm memory, construct a graph schedule, or free an unbounded object graph.
- Define recording-capacity behavior explicitly. Low storage must be reported before exhaustion; exhaustion must stop/refuse recording safely rather than allocate unpredictably or overwrite data.
- Publish live state at a bounded display cadence independent of the 128-frame callback rate. Transfer waveform data only on revisioned request, in bounded chunks, and never once per render quantum.
- Request microphone constraints with echo cancellation, noise suppression, and automatic gain control disabled where supported, but inspect/report actual settings and tolerate browsers that ignore optional constraints.
- Begin permission and context startup from a user gesture. Never try to bypass autoplay policy, auto-grant permission, or hide denial behind a silent dummy fallback.
- Keep microphone tracks, source node, worklet node, destination connection, context, callbacks, and object URLs alive for one explicit lifecycle owner. Shutdown stops tracks, disconnects nodes, closes the context, revokes URLs, and ignores late completions by generation.
- Treat feedback as a user-safety concern: monitoring defaults to the existing off state, the UI identifies when microphone monitoring is live, and tests use virtual/fake devices.
- Keep native feature selection target-specific. Browser code must not change native driver behavior or make native crates depend on `web-sys`.
- Do not commit generated `dist` artifacts. Keep the worklet build reproducible from source and include it in release and CI commands.
- Preserve all completed Milestone 2 evidence as historical evidence; add Milestone 3 rows and status instead of rewriting the prior boundary.

## Staged implementation

### Stage 1 — Freeze the browser audio, worklet ABI, and packaging contracts

No later stage may rely on synchronous worklet replies or treat a main-thread prototype as an AudioWorklet implementation.

- [x] Add and refine Milestone 3 matrix entries for permission lifecycle, Web Audio routing, worklet ownership, asynchronous backend control, real-time storage, browser artifacts, and native isolation.
- [x] Inventory every thread, wait, mutex, allocation exception, graph rebuild, object creation, content snapshot, and direct-session read reachable from the current browser backend; classify it as worklet-safe, control-side, replaced, or unavailable.
- [x] Define the browser driver lifecycle and generation model: unsupported, awaiting gesture, requesting permission, starting worklet, running, suspended, denied, failed, stopping, and stopped.
- [x] Define the versioned typed command/event ABI, stable-ID ownership, queue capacities, control-task policy, status cadence, waveform chunk bounds, and protocol mismatch behavior.
- [x] Define default mono/stereo capture and playback mapping, multi-track summing/clipping policy, monitoring defaults, and behavior when the browser exposes fewer channels than requested.
- [x] Define recording-pool limits, low-capacity reporting, exhaustion behavior, and memory-growth policy before implementing capture.
- [x] Prove the direct-`web-sys` bootstrap with fake/default microphone permission, one context, a dedicated Wasm module, a minimal AudioWorklet shim, and non-zero microphone-to-destination callbacks; the proven bootstrap was retained as production code rather than discarded.
- [x] Prove the selected worklet packaging loads under Trunk on HTTPS/localhost without CPAL, Firewheel, SharedArrayBuffer, or main-thread sample transport; document the evidence-backed serialized control-task design revision.
- [x] Record the accepted architecture, secure-context/offline artifact behavior, and dependencies in the project document and matrix.

Verification:

- A headless browser with fake media grants permission only after the scripted enable action and reports non-zero input and output callback metrics.
- A denied-permission run remains responsive, shows the denied state, and can retry.
- Artifact and dependency inspection proves the spike uses repository-owned worklet code and no browser audio abstraction.
- The worklet ABI/limits document is specific enough to implement and test without synchronous browser waits.

Commit the contract and proven worklet bootstrap before adapting engine or application ownership.

### Stage 2 — Make the engine core safe for worklet-owned full-duplex processing

Depends on Stage 1.

- [x] Extract target-neutral backend IDs, command/event values, and engine-facing direct-track logic from the current dummy façade so native dummy and worklet hosts share behavior without sharing runtime ownership.
- [x] Add worklet-local audio ports that stage microphone channel slices and expose output slices without mutexes, capture queues, or per-cycle allocation.
- [x] Add a full-duplex engine primitive that stages input, processes exactly the supplied frame count, and mixes output using the frozen routing contract; bounded commands run in serialized non-render control tasks.
- [x] Replace the 64-sample browser channel configuration with prepared/reusable recording storage that meets the frozen capacity contract and never allocates during sustained recording.
- [x] Prepare topology, schedules, port buffers, scratch, and channel storage before the next render callback while callbacks continue against the last installed schedule.
- [x] Ensure replaced commands, schedules, snapshots, channels, and recording chunks are recycled or destroyed outside `process()` with bounded work.
- [x] Add bounded state/meter publication and revisioned waveform extraction primitives suitable for chunked worklet events.
- [x] Add callback-budget/discontinuity, command overflow, stale graph, storage low/exhausted, memory-growth, input/output activity, and xrun counters.
- [x] Keep MIDI data structures inert or omitted in this milestone without enabling Web MIDI or native Midir.
- [x] Preserve native dummy and full native application-backend behavior through feature-separated adapters.

Verification:

- Native deterministic harnesses drive 128-frame mono and stereo input through record, monitor, stop, and playback and observe exact non-zero outputs.
- Real-time guard tests cover warmed and sustained cycles, command bursts, loop growth up to the configured capacity, graph changes, waveform publication, pool exhaustion, and teardown without render-path allocation or unapproved locks.
- Queue/pool exhaustion and stale commands produce deterministic events while later valid transport commands still progress.
- Existing engine no-allocation, realtime-lock, dummy, JACK/CPAL test-backend, and full-feature suites pass.

Commit the worklet-safe engine primitive and storage/scheduling limits before browser protocol integration.

### Stage 3 — Build the dedicated worklet module and bounded protocol

Depends on Stages 1–2.

- [x] Create a dedicated Wasm worklet package/artifact containing only the protocol host, worklet-safe backend core, and required `shoop_engine` features.
- [x] Expose a narrow versioned ABI for initialization, bounded command submission, one render quantum, status extraction, event extraction, waveform chunks, and shutdown.
- [x] Implement the minimal AudioWorkletProcessor shim that instantiates the supplied module, preallocates/reuses planar views, forwards bounded commands, calls the Rust quantum function, writes output, and remains alive only while the Rust host is healthy.
- [x] Apply message commands in serialized non-render control tasks, as documented in the design revision, so topology/JSON allocation never enters `process()`.
- [x] Throttle status/events independently from audio callbacks and transfer requested waveform chunks with explicit revision, offset, length, final, and cancellation fields.
- [x] Detect ABI mismatch, Wasm trap, memory-view invalidation, queue overflow, processor termination, and malformed messages and report them to the main-side controller.
- [x] Make the worklet module deterministic to initialize and destroy repeatedly without leaked processors, object URLs, media tracks, or contexts.
- [x] Integrate worklet artifact production with local build, Trunk, release, and CI without committing generated files.

Verification:

- Protocol conformance tests replay valid, stale, duplicate, out-of-order, saturated, malformed, and shutdown sequences.
- `wasm-tools`/module inspection confirms the worklet artifact contains no eframe, DOM, native-driver, thread-worker, filesystem, CPAL, Firewheel, Midir, JACK, or LV2 path.
- A browser harness runs sustained 128-frame fake input while issuing controls and topology changes; callbacks and status continue, output remains finite, and counters stay within defined limits.
- Repeated create/start/stop/drop cycles release microphone indicators and stop callback revisions.

Commit the independently tested worklet artifact before wiring it to the application model.

### Stage 4 — Add the Web Audio controller and asynchronous backend adapter

Depends on Stage 3.

- [x] Add target-gated `web-sys` features and a browser controller that creates/resumes one `AudioContext`, requests the default microphone with raw-audio preferences, reports actual track settings, loads the worklet module, creates/connects nodes, and owns cleanup.
- [x] Start microphone/context work synchronously from the enable-audio gesture before awaiting permission, while keeping all later Promise results generation-safe.
- [x] Implement the lifecycle state machine, retry, page/context suspension handling, media-track-ended handling, worklet failure handling, and explicit offline/dummy choice for unsupported or denied environments.
- [x] Implement a `WebAudioBackend` proxy whose synchronous methods only validate and submit bounded commands; completions, failures, live state, and waveform chunks arrive through polling without blocking.
- [x] Refactor the backend/application contract for pending asynchronous mutation while preserving ordered intents, stable IDs, visible failure, retry-journal reconciliation, and immediate native completion.
- [x] Remove `Send` from the abstract backend contract and require `Send` specifically at the native threaded runtime boundary.
- [x] Ensure browser startup does not construct or advance `EngineBackend::new_dummy` once physical audio is selected; retain an explicit separately tested offline dummy path.
- [x] Add plain API status/capability values and presentation for permission, live monitoring, context/driver state, actual rate/quantum, activity, errors, and retry without adding Web Audio dependencies to `shoop_egui`.
- [x] Keep dispatch and polling bounded when the worklet is slow, suspended, saturated, or gone.

Verification:

- Application tests cover pending/acknowledged/failed creation, controls, stale IDs, queue saturation, retry generations, delayed waveform chunks, and worklet loss using a deterministic proxy transport.
- Browser tests cover granted, denied, dismissed/failed, retry, unsupported secure context, suspend/resume, track end, and shutdown states without console exceptions or hidden fallback.
- Native actor tests observe unchanged immediate backend behavior and shutdown.
- Wasm dependency inspection shows Web Audio only in the runner/backend controller side and engine DSP only in the worklet artifact.

Commit asynchronous application/backend integration before replacing the browser composition path.

### Stage 5 — Select Web Audio automatically in the unified browser application

Depends on Stage 4.

- [x] Replace browser dummy startup with the enable-audio/lifecycle state and automatic Web Audio backend selection; leave native `Runtime::new()` and its threaded dummy backend unchanged.
- [x] Keep the eframe application responsive before permission, during Promise resolution, while suspended, and after failure; initialize authoritative application state exactly once per successful driver generation.
- [x] Route current direct-track creation, controls, monitoring, loop record/stop/play, selection/details, global controls, and waveform requests through the worklet-backed engine.
- [x] Update browser status attributes/logging so automation can distinguish permission, context, callback, input, output, engine revision, xrun, and self-test state without using status text as sole evidence.
- [x] Extend the browser self-test to click enable, use deterministic non-silent fake capture, create mono and stereo tracks, monitor, record, stop, verify non-zero waveform content, play, verify non-zero output, and prove callback/state revisions continue.
- [x] Add permission-denied, context-suspended/resumed, worklet-failure, repeated-start prevention, minimum/common viewport, and cleanup browser scenarios.
- [x] Preserve the self-contained artifact as an explicit offline/dummy experience or a precise secure-context limitation according to the Stage 1 contract; never label it microphone-capable when opened from `file:`.
- [x] Update browser titles, status, README/run instructions, Trunk configuration, local server guidance, artifact builder, and workflow names/descriptions from dummy-only to target-specific audio behavior.
- [x] Update the project document and parity matrix with implementation status and evidence.

Verification:

- `cargo run -p shoopdaloop_egui` still starts the native threaded dummy application.
- `cargo check -p shoopdaloop_egui --target wasm32-unknown-unknown` and the dedicated worklet build succeed.
- Hosted Trunk browser runs automatically choose Web Audio after the enable gesture; no driver-selection UI is needed.
- Headless Chrome fake-media workflows at 360×200 and 900×600 prove permission, callbacks, non-zero capture/recording/waveform/playback/output, controls, and continued responsiveness.
- Denial/retry, suspension/recovery, and teardown scenarios pass and leave no active media track after shutdown.

Commit automatic browser selection and packaging/documentation as meaningful reversible milestones.

### Stage 6 — Complete cross-target validation and evidence

Depends on all prior stages.

- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build`.
- [x] Run targeted `shoop_engine`, `shoop_backend`, `shoop_app`, `shoop_egui`, unified-runner, protocol, worklet-host, real-time lock, no-allocation, and tracing tests.
- [x] Run the serialized full workspace suite with `SHOOP_ALLOW_MISSING_BACKENDS=1` and `shoop_engine/app_backend` enabled as required by the environment.
- [x] Build and run the retained offscreen Qt/QML self-tests and record the environment-only CPAL virtual-port skip without weakening native regression requirements.
- [x] Inspect native, browser-main, and worklet dependency/module graphs for target leakage and confirm `shoop_egui` remains implementation-backend-free.
- [x] Build release hosted and self-contained artifacts; test hosted physical audio and the documented offline/secure-context behavior separately.
- [x] Run browser granted/denied/retry/suspend/worklet-failure/cleanup suites plus non-zero audio workflows at minimum and common viewport sizes.
- [x] Run a sustained fake-media stress test with recording, topology construction, UI repaint pressure, and lifecycle changes; assert callback budget, queue, storage, xrun, and memory-growth policies.
- [x] Run current Chrome and Firefox deterministic microphone/output checks and record the unavailable physical-hardware environment and untested Safari compatibility limitation without making unsupported claims.
- [x] Confirm native egui dummy startup/workflow and retained native real-driver suites have no regression.
- [x] Confirm every Milestone 3 matrix row has accurate discovery, implementation, and evidence without altering Milestone 1 or 2 completion history.
- [x] Update `EGUI_REPLACEMENT_PROJECT.md`, `EGUI_FEATURE_PARITY_MATRIX.md`, root/package documentation, CI descriptions, and this plan with final status and evidence.
- [x] Commit the completed implementation and final validation documentation.

Final validation demonstrates that the hosted browser application selects a direct Web Audio/AudioWorklet backend, obtains microphone permission through a user gesture, records and plays non-zero fake audio through authoritative Shoop engine state with bounded real-time behavior, handles lifecycle failures visibly, packages all worklet assets reproducibly, and leaves every native path operational.

## Completion evidence

- `cargo fmt --all -- --check` and `RUSTFLAGS="-D warnings" cargo build` pass.
- Targeted protocol/worklet/backend/application/presentation/runner and full `shoop_engine` suites pass, including allocation-guarded 128-frame full duplex and bounded-storage exhaustion.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1` passes, including retained JACK test-backend paths.
- `QT_QPA_PLATFORM=offscreen SHOOP_ALLOW_MISSING_BACKENDS=1 target/debug/shoopdaloop_dev.sh --self-test` reports 236 passed, 0 failed, and one CPAL virtual-port environment skip.
- Release Trunk and self-contained builds pass. Chrome 147 passes 360×200, 900×600, denial/retry, suspension/recovery, worklet loss/retry, shutdown, 1,500-callback stress, explicit offline, and direct-file secure-context-limitation scenarios. Firefox 150 passes the non-zero fake-media workflow at 900×600.
- Worklet dependency inspection excludes browser abstractions, native drivers, frontend, Qt, and window-system crates. `WebAssembly.Module.imports()` returns an empty list for the worklet artifact.
- The validation host has no `/dev/snd`; no physical microphone/headphone claim is made. Safari remains untested. The immutable criteria permit deterministic fake-media I/O evidence, which is used here.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
