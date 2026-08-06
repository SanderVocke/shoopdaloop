# Milestone 3 Completion Audit

This audit maps every requirement in `EGUI_MILESTONE_3_BROWSER_AUDIO.md` to implementation and verification evidence. It supplements the checked plan; it does not change the immutable acceptance criteria.

## Objective and success criteria

The objective is complete only when:

1. all 16 immutable browser-audio acceptance criteria are implemented;
2. every Stage 1–6 implementation and verification item has concrete evidence;
3. the named formatting, build, Rust, browser, artifact, dependency, and retained-native gates pass;
4. `EGUI_FEATURE_PARITY_MATRIX.md` and `EGUI_REPLACEMENT_PROJECT.md` accurately describe the resulting scope and limitations; and
5. generated artifacts are excluded and the implementation is committed without disturbing unrelated work.

## Immutable acceptance-criteria audit

| Criterion | Implementation evidence | Direct verification |
|---|---|---|
| 1. Target-selected Web Audio; native dummy preserved | Target-gated `Runtime` in `src/rust/shoopdaloop_egui/src/main.rs`; hosted mode constructs `WebAudioBackend`, `?offline=1` alone constructs `EngineBackend::new_dummy`; native mode remains `ApplicationRuntime` plus threaded dummy | Hosted Chrome reports `Running`; offline test reports `Dummy`; `native_dummy_workflow_creates_records_and_controls_tracks_and_loops` passes |
| 2. Explicit gesture, permission, visible failure/retry | `#enable_audio` handler and synchronous `begin_enable` in `browser_audio.rs` create/resume the context and invoke `getUserMedia` before spawning asynchronous completion; unsupported/denied/failed states remain retryable | Chrome normal and `DENY_FIRST=1` scenarios pass; direct-file limitation test reports the HTTPS/localhost requirement |
| 3. One microphone → worklet → destination graph at actual rate/quantum | `start_audio_graph` creates one `AudioContext`, `MediaStreamAudioSourceNode`, `AudioWorkletNode`, and destination connection; host is initialized with `context.sample_rate()` and receives callback length | Chrome and Firefox report non-zero I/O, actual sample rate, and 128-frame callbacks |
| 4. Worklet-exclusive engine ownership and clock | `shoop_audio_worklet::WorkletHost` privately owns `EngineBackend`; only `shoop_worklet_process` calls `process_audio_quantum`; `WebAudioBackend::advance` is a no-op | Callback/frame counters advance in browser tests, stop while shut down, and recover without elapsed-time catch-up |
| 5. Deterministic mono/stereo routing and safe monitoring | `EngineBackend::process_audio_quantum` and `BROWSER_AUDIO_CONTRACT.md` define capture mapping, mono duplication, stereo mapping, summing, clipping, and monitoring-off default | Backend full-duplex test and Chrome self-test create mono/stereo tracks and prove monitored non-zero destination output |
| 6. Non-zero record/waveform/playback workflow | `BrowserSelfTest` creates tracks, enables monitoring, records, stops, requests waveform chunks, and plays | Chrome at 360×200/900×600 and Firefox at 900×600 require positive input/output peaks and non-zero waveform completion |
| 7. Asynchronous bounded protocol | `shoop_audio_protocol` defines versioned typed envelopes/stable IDs; `Transport` bounds journal, in-flight commands, and inbound events at 256, coalesces controls, and never waits | Protocol ordering/malformed/oversize tests pass; `SATURATE=1` observes overflow, retained callback progress, and running-state recovery |
| 8. Real-time-safe render path | `audio_worklet.js::process` performs indexed copies, one Rust call, memory checks, and timer reads; it does not parse JSON, mutate topology, post messages, wait, lock, or allocate arrays/views in steady state | Allocation-guarded 128-frame worklet/backend and engine tests pass; stress reports zero unexpected budget overruns and memory growth |
| 9. Prepared bounded topology/storage/lifetimes | Control tasks build topology outside `process`; external ports and session scratch are pre-sized; `ChunkedSamples` has a hard ten-second prepared capacity and safe exhaustion | Bounded-storage exhaustion/no-allocation tests and 1,500-callback browser recording stress pass |
| 10. Observable diagnostics | `BackendStatus`, `AppStatus`, egui status, and DOM attributes expose driver/context state, callbacks, rate, quantum, peaks, xruns, discontinuities, measured callback-budget overruns, queue overflow, storage, memory growth, generation, and owned media tracks | Browser tests assert rate/quantum/activity, zero unexpected counters, overflow on intentional saturation, and media ownership transitions |
| 11. Lifecycle, retry, and cleanup | Generation checks cover promises and callbacks; repeated starts are rejected; track-end/worklet/context handlers fail visibly; failed generations release graphs; shutdown stops tracks, detaches handlers, closes ports/context, and removes global listeners | `LIFECYCLE=1` covers suspend/resume, media-track end/retry, processor loss/retry, shutdown, stopped callbacks, zero owned media tracks, and visible retry; every hosted test checks repeated-start generation stability |
| 12. Browser dependency isolation | Worklet crate links protocol/backend/engine only; Web Audio is target-gated in composition; `shoop_egui` remains presentation-only | Cargo-tree forbidden-package scans pass; `WebAssembly.Module.imports()` returns `[]`; warning-free native build passes |
| 13. Reproducible hosted and self-contained assets | `Trunk.toml` invokes `build_worklet.py --locked`; source shim and generated worklet are copied into hosted output; `build_single_file_app.py` embeds UI assets; generated files are ignored | Release Trunk build and single-file builder pass; hosted audio, explicit `?offline=1`, and direct-file secure-context-limitation tests pass |
| 14. Native behavior retained | Browser code is `wasm32`-gated; native `Runtime::new` is unchanged in architecture; full native drivers remain in retained feature paths | Warning-denying workspace build, native runner tests, serialized full workspace with `shoop_engine/app_backend`, JACK test paths, and QML suite pass |
| 15. Required automated coverage | Unit/integration tests cover protocol, worklet host, engine full duplex, bounded storage, allocation/lock guards, application/presentation/native runner; browser automation covers permission, I/O, lifecycle, saturation, stress, viewports, and artifacts | Targeted suites pass; Chrome and Firefox fake-media workflows pass; deterministic sine capture, peaks, waveform samples, and output—not status text—are asserted |
| 16. Accurate target-specific documentation | Root/package README, browser HTML, CI names, contract, plan, matrix, and project document distinguish native dummy, hosted Web Audio, permission/secure-context needs, explicit offline dummy, and unavailable Web MIDI | Source/document scan plus all 13 `M3-*` matrix rows show `Complete`; plan has no unchecked item |

## Staged prompt-to-artifact checklist

### Stage 1 — Contract and bootstrap

1. Matrix entries: M3 section and 13 `M3-*` rows in `EGUI_FEATURE_PARITY_MATRIX.md`.
2. Portability inventory: `BROWSER_AUDIO_CONTRACT.md#portability-inventory`.
3. Lifecycle/generations: contract lifecycle section and `BrowserControllerInner`.
4. ABI/bounds: `shoop_audio_protocol` plus contract protocol section.
5. Routing/mixing: contract routing section and backend full-duplex implementation/tests.
6. Recording limits: contract storage section and bounded `ChunkedSamples` APIs/tests.
7. Direct `web-sys` proof: retained production controller plus Chrome/Firefox fake-media runs.
8. Trunk/isolation proof: `Trunk.toml`, `build_worklet.py`, cargo-tree scan, raw no-import module check.
9. Architecture/artifact record: contract, project document, and matrix.

### Stage 2 — Worklet-safe engine

1. Shared IDs/control values: `shoop_backend` IDs and `shoop_audio_protocol` wire values.
2. Worklet-local ports: `ExternalAudioPort` staging/output slices.
3. Exact-frame full duplex: `EngineBackend::process_audio_quantum` and 128-frame tests.
4. Prepared recording storage: bounded-capacity channel/session APIs and ten-second backend allocation.
5. Prepared topology/scratch: synchronous control-task graph installation and session pre-sizing.
6. Off-render replacement/destruction: serialized message tasks and controller cleanup.
7. Bounded state/waveform: 50 ms snapshots and 512-sample revisioned chunks.
8. Diagnostics: expanded backend/application status and browser attributes.
9. Inert MIDI: no Web MIDI API/dependency; browser reports `unavailable`.
10. Native adapters: dummy backend and retained full `app_backend` tests.

### Stage 3 — Dedicated worklet and protocol

1. Dedicated package: `src/rust/shoop_audio_worklet`.
2. Narrow ABI: exported create/destroy/input/output/command/response/process functions.
3. Minimal shim: `audio_worklet.js` with pre-bound exports and reusable views.
4. Non-render command tasks: `handleCommand` calls host only from `MessagePort` tasks.
5. Throttled status/chunking: `STATUS_INTERVAL_MS`, waveform revisions/offset/final fields.
6. Visible malformed/trap/growth/termination errors: protocol host, shim, and controller error paths.
7. Repeatable lifecycle: generation replacement, track stop, node/port/context teardown, listener removal.
8. Build integration: Trunk hook, release build, ignored generated directory, CI.

### Stage 4 — Controller and async adapter

1. Target-gated Web APIs/raw settings: runner Cargo features and `browser_audio.rs` capture constraints/settings attributes.
2. Gesture-sensitive start: synchronous context/resume/getUserMedia calls in `begin_enable`.
3. Lifecycle/retry/offline: controller states and explicit runtime mode selection.
4. Non-blocking proxy: `WebAudioBackend` submits/polls `Transport`; no synchronous reply path.
5. Ordered intents/stable IDs/reconciliation: application-assigned IDs, journal replay/coalescing, typed errors.
6. `Send` boundary: abstract `Backend` is non-`Send`; native actor requires `Send` at its thread boundary.
7. No hosted dummy progression: physical proxy `advance` is empty; only explicit offline constructs dummy.
8. Plain status/presentation: `shoop_app_api`, `shoop_app`, `shoop_egui`, and DOM diagnostics.
9. Bounded slow/gone behavior: in-flight/event capacities, poll threshold/cadence, observable overflow/failure.

### Stage 5 — Unified browser application

1. Automatic target selection: `Runtime::new` target/mode branches.
2. Responsive single-generation app: cooperative pump plus generation-safe asynchronous startup.
3. Existing workflow routing: application intents become protocol track/loop/control/waveform commands.
4. Automation diagnostics: non-text DOM attributes in `set_browser_status` and controller presentation.
5. Non-zero self-test: `BrowserSelfTest` and browser fake sine source.
6. Failure/viewport/cleanup tests: Chrome normal, denial, lifecycle, saturation, stress, 360×200/900×600.
7. Explicit direct-file behavior: offline and secure-limit branches in `browser_smoke.mjs`.
8. Packaging/run text: root/package README, `index.html`, Trunk, builders, CI.
9. Project/matrix updates: current M3 sections and status table.

### Stage 6 — Named gates and deliverables

1. `cargo fmt --all -- --check`: passes.
2. `RUSTFLAGS="-D warnings" cargo build`: passes.
3. Targeted engine/backend/app/presentation/runner/protocol/worklet/real-time/tracing suites: pass.
4. `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`: passes.
5. Offscreen QML self-test: 236 passed, 0 failed, one environment-only CPAL virtual-port skip.
6. Dependency/module inspection: forbidden-package scans pass and worklet imports are empty.
7. Release hosted/single-file builds: pass; hosted/offline/direct-file modes tested separately.
8. Browser grant/deny/retry/suspend/track-loss/worklet-loss/cleanup/viewports: pass.
9. Sustained fake-media stress: exceeds 1,500 callbacks with non-zero recording/output and zero unexpected render diagnostics.
10. Chrome/Firefox compatibility: Chrome 147 and Firefox 150 pass deterministic fake-media I/O; physical hardware and Safari are explicitly not claimed.
11. Native regression: native dummy tests and retained native real-driver/test-backend suites pass.
12. Matrix audit: all 13 M3 rows are `Complete`; M1/M2 history remains present.
13. Documentation: project, matrix, plan, contract, README files, HTML, and CI are current.
14. Commit: implementation commit `98da2de2`; lifecycle/audit hardening is committed with this audit.

## Current verification record

The completion run produced the following direct evidence:

- Chrome 147: hosted 360×200 and 900×600, denied permission/retry, repeated-start rejection, context suspend/resume, media-track end/retry, forced worklet loss/retry, intentional 256-command saturation/recovery, shutdown with zero owned media tracks, and sustained recording (over 9,000 observed callbacks) all passed.
- Firefox 150.0.1: hosted 900×600 reported `Running`, 248 callbacks, non-zero input/output, 128-frame quantum, one owned media track, zero overflow, and zero measured callback-budget overrun.
- Self-contained artifact: `?offline=1` selected explicit dummy operation; direct `file:` without it reported the precise secure-context limitation.
- Current targeted Rust suites: protocol 2/2, worklet host 3/3, unified runner 3/3; Wasm warning-denying check and formatting pass.
- The implementation gate additionally passed the warning-denying workspace build, serialized full workspace suite, retained JACK paths, engine real-time tests, and offscreen QML 236/0 suite noted above. Subsequent hardening changed only browser controller/shim/automation/docs and was rechecked on both browser engines.

## Limitations and audit result

`/dev/snd` is unavailable on the validation host, so no physical microphone/headphone claim is made. Safari is untested. Both limitations are recorded consistently and deterministic fake-media I/O is explicitly permitted by the immutable criteria.

No mandatory Milestone 3 requirement remains missing or weakly covered after adding direct media-track-loss, repeated-start, queue-saturation/recovery, owned-track cleanup, and measured callback-budget evidence. The objective is achieved.
