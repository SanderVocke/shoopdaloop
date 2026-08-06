# Carla subprocess acceptance evidence

This audit maps the immutable `PLAN.md` requirements to implementation artifacts and reproducible verification surfaces. A requirement is complete only when its listed evidence exists and, for platform-specific requirements, has run on the named operating system.

## Verification commands

| ID | Command / surface | Purpose |
|---|---|---|
| G1 | `cargo test --workspace --features shoop_engine/app_backend` | Full native Rust workspace regression gate |
| G2 | `cargo check --workspace --all-targets --features shoop_engine/app_backend` | All native workspace targets and examples |
| G3 | `cargo test -p shoop_engine --features app_backend --test no_alloc --test no_std_mutex` | Realtime allocation and lock source/runtime guards |
| G4 | `cargo check -p shoop_engine --target wasm32-unknown-unknown --no-default-features && cargo check -p shoopdaloop_egui --target wasm32-unknown-unknown` | Browser/Wasm exclusion and egui compatibility |
| G5 | `SHOOP_ALLOW_MISSING_BACKENDS=1 QT_QPA_PLATFORM=offscreen target/debug/shoopdaloop_dev.sh --self-test` | Full QML behavior/session regression gate; unavailable virtual hardware is explicitly reported |
| G6 | `cargo test -p shoopdaloop --test carla_worker -- --nocapture --test-threads=1` | Real and Carla-independent worker lifecycle, failure, cleanup, benchmark, and transport integration |
| G7 | `cargo fmt --all -- --check && git diff --check` | Formatting and patch hygiene |
| G8 | Build a Linux portable folder with `package build-portable-folder`, place it in `/tmp/Shoop Package current ü space`, select subprocess mode in `settings.json`, then run its launcher with `--self-test --test-files-pattern <absolute .../tst_TrackControlAndLoop_drywet_carla.qml>` | Installed executable discovery, quoted path handling, self-spawn, packaged dependencies, and six real Carla dry/wet cases |
| G9 | `cargo build --release -p shoopdaloop --bin shoopdaloop -p shoop_engine --example carla_bridge_benchmark --features shoop_engine/app_backend`, followed by `carla_bridge_benchmark <worker> direct`, `subprocess`, and `reference` | 500-block paced Linux production benchmark recorded in `CARLA_SUBPROCESS_BENCHMARK.md` |
| G10 | `.github/workflows/build_and_test.yml` release build/test matrix on Linux x64, Windows MSVC x64, macOS Intel, and macOS ARM; Rust tests emit and `.github/actions/test_toplevel/action.yml` uploads `carla-subprocess-benchmarks-*` CSV artifacts; each portable package also runs the six-case Carla QML file with the test-only startup override selecting subprocess mode | Native package/runtime/lifecycle/deadline, user-visible QML semantics, and direct/subprocess benchmark evidence on all supported OS families |

## Requirement-to-evidence matrix

| Requirement | Implementation artifacts | Tests / gates | Status |
|---|---|---|---|
| REQ-01 | `src/rust/shoop_settings/src/lib.rs`; `src/session_schemas/schemas/settings.1.json`; `src/qml/SettingsWindow.qml` | settings persistence tests; QML selector tests; G1/G5 | Verified |
| REQ-02 | `src/rust/shoop_engine/src/app_backend.rs`; `src/rust/shoop_engine/src/lv2_carla.rs` | direct six-case `tst_TrackControlAndLoop_drywet_carla.qml`; Carla engine tests; G1/G5 | Verified |
| REQ-03 | `src/qml/SettingsWindow.qml`; `docs/source/usage.carla_subprocess.rst` | UI binding/help coverage; G5 | Verified |
| REQ-04 | `src/rust/shoop_settings/src/lib.rs` compatibility default and merge-save | absent/old/malformed/unknown-key/save-reload tests; G1 | Verified |
| REQ-05 | `SupervisedCarlaProcessor` and per-chain launch in `carla_subprocess.rs` / `app_backend.rs` | `fake_supervisor_restarts_saves_while_down_and_isolates_chains`; `separate_chains_use_independent_worker_processes`; G6 | Verified |
| REQ-06 | worker entry in `shoopdaloop/src/lib_impl.rs`; `CarlaLv2Host` ownership in `carla_subprocess.rs` | real worker process/state/audio/MIDI and external-UI tests; G6 | Verified |
| REQ-07 | child process boundary; panic containment; one-period fallback in `carla_subprocess.rs` and `carla_processor.rs` | fake abort/process-error tests, child-kill tests, bridge panic/deadline tests, sibling survival; G3/G6 | Verified |
| REQ-08 | requested shutdown and UI-close classification in `carla_subprocess.rs`; notification dedup in `FXChain.qml` | requested reap, UI show/hide/close, unload/drop, shutdown-soak tests; G5/G6 | Verified |
| REQ-09 | `SharedWorkerGuard`, bounded kill/reap, `SharedMemoryCleanup`, generation temp mappings | `requested_worker_shutdown_reaps_and_removes_shared_memory`; `worker_exits_and_cleans_ipc_after_abnormal_parent_termination`; shutdown soak; G6/G10 | Verified on Linux; all-platform runtime evidence is part of REQ-27 |
| REQ-10 | fixed-layout `carla_shared_memory.rs`; preallocated parent/worker MIDI pools | shared-slot audio/MIDI tests and allocation guards; G1/G3/G6 | Verified |
| REQ-11 | unique `CarlaRealtimeProcessor`; atomic snapshots; bounded waits; control thread separation | `no_std_mutex`, `no_alloc`, full bridged session guard; G3 | Verified |
| REQ-12 | fixed MIDI event/byte capacities and offsets in protocol/shared mapping | protocol offset/byte tests, fake 16-channel round trip, overflow counters and QML gating; G1/G6 | Verified |
| REQ-13 | three-slot ownership states, abandoned-slot reclamation, wet-silence/MIDI-drop fallback | timeout/reuse/stale tests, fake hang/deadline test, later-block recovery, sibling test; G1/G6 | Verified |
| REQ-14 | shared memory + authenticated UDP wake; framed TCP control only | G9 shared-memory/reference comparison; G6 fixture/real matrices | Verified |
| REQ-15 | constants and ownership protocol in `shoop_plugin_protocol` / `carla_shared_memory.rs`; metrics and Tracy plots | capacity, malformed, race, timeout, stress, and deadline tests; G1/G3/G6 | Verified |
| REQ-16 | supervisor-owned `checkpoint` independent of `current` worker | supervisor crash/save/restart tests; G6 | Verified |
| REQ-17 | checkpoint replacement only after successful `save_state` / `restore_state` | state-failure and repeated-generation tests; G1/G6 | Verified |
| REQ-18 | unavailable-worker save returns supervisor checkpoint | `fake_supervisor_restarts_saves_while_down_and_isolates_chains`; G6 | Verified |
| REQ-19 | `toggle_or_recover` generation/instantiate/restore/active/show sequence | supervisor repeated restart plus QML recovery adapter tests; G5/G6 | Verified |
| REQ-20 | unavailable lifecycle, preserved checkpoint/logs, crash summary | startup/restore/UI failure tests and red unavailable UI state; G1/G5/G6 | Verified |
| REQ-21 | immediate independent pipe drains and `BoundedLog` | binary flood, fake child pipe flood, truncation/dropped-byte tests; G1/G6 | Verified |
| REQ-22 | generation stdout/stderr panes in `src/qml/FXChain.qml` | inspect/copy/clear/refresh/truncation UI tests; G5 | Verified |
| REQ-23 | bounded `previous_logs` generation deque | four-generation retention assertion and QML generation labels; G5/G6 | Verified |
| REQ-24 | `last_notified_crash_generation` and log action in `FXChain.qml` | crash-generation dedup tests; G5 | Verified |
| REQ-25 | lifecycle snapshots and state colors/tooltips in `TrackWidget.qml` | QML lifecycle/status tests; G5 | Verified |
| REQ-26 | portable std/TCP/UDP/tempfile/memmap implementation; same target-neutral lifecycle/UI model | G10 package, Rust, and QML results on Windows/Linux/macOS | **Pending successful G10 evidence** |
| REQ-27 | nonce/version/framing validation; random temp mapping; loopback launch/control; disconnect cleanup; bounded timeout/kill | protocol/shared tests plus the complete fake child-process matrix and abnormal-parent cleanup in G6, executed by G10 on all OS families | **Pending successful G10 evidence** |
| REQ-28 | self-spawn via `current_exe`; portable `shoopdaloop_exe`; quoted Linux launchers | G8 passed from a non-ASCII path containing spaces; G10 package smoke | Verified (Linux fixture and package architecture) |
| REQ-29 | worker-owned `CarlaLv2Host` handles both process and UI requests | real subprocess external-UI show/hide under Xvfb; G6 opt-in UI test | Verified |
| REQ-30 | `CarlaProcessor`, control/realtime bridge, protocol crate, `SharedBlockTransport`, frontend-neutral snapshots | serialized reference and shared transport use the same high-level interfaces; G1/G2/G4/G9 | Verified |
| REQ-31 | protocol/shared transport tests; fake worker modes; real worker and QML suites | malformed/flood/abort/error/hang/timeout/restart/save-down/multi-chain/cleanup matrix in G1/G5/G6 | Verified |
| REQ-32 | allocation guards in `shoop_engine/tests/no_alloc.rs`; lock source guards in `no_std_mutex.rs` | G3 | Verified |
| REQ-33 | G9 Linux production report; real-Carla and deterministic fixture matrices in `shoopdaloop/tests/carla_worker.rs` emit p50/p95/worst/misses for 2/16 channels and 32–1,024 frames | G10 archives per-platform CSV files | **Pending Windows/macOS G10 measurements** |
| REQ-34 | `usage.carla_subprocess.rst`; `developers.software.rst`; benchmark/baseline reports; parity notes | docs build/package inclusion and artifact inspection | Verified |

## Deliverables and boundaries

- Acceptance contract and progress: `PLAN.md`.
- Baseline inventory and measurements: `CARLA_SUBPROCESS_BASELINE.md`.
- Transport benchmark, mechanism audit, and rejected alternatives: `CARLA_SUBPROCESS_BENCHMARK.md`.
- Protocol/value crate: `src/rust/shoop_plugin_protocol`.
- Settings service: `src/rust/shoop_settings`.
- Processor, worker, supervisor, and shared transport: `src/rust/shoop_engine/src/carla_processor.rs`, `carla_subprocess.rs`, and `carla_shared_memory.rs`.
- Current frontend integration: `src/qml/FXChain.qml`, `SettingsWindow.qml`, and `TrackWidget.qml` plus CXX-Qt adapters.
- User/developer documentation: `docs/source/usage.carla_subprocess.rst` and `docs/source/developers.software.rst`.
- Future frontend contract: `EGUI_FEATURE_PARITY_MATRIX.md`; browser/Wasm remains explicitly out of native hosting scope.

The only open evidence in this audit is native Windows/macOS execution and measurement. Linux-only compilation or timing is not used to satisfy those rows.
