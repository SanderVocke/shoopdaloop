- [x] Inventory and document all remaining production dependencies on C++ `AudioMidiDriver` class/inheritance, including factory, API dispatch, and backend implementations. (See `docs/audio_midi_driver_rust_migration_inventory.md`.)

- [x] Define and document a strict behavior-parity checklist (process order, wait semantics, dummy mode semantics, telemetry fields, API error behavior) to gate each migration phase. (Added to `docs/audio_midi_driver_rust_migration_inventory.md`.)
- [ ] Extend Rust driver API in `backend_rust` to cover complete lifecycle and backend metadata (create/destroy/start/close/wait/type).
- [ ] Extend Rust driver API to cover port operations and backend-specific controls needed by current C API (audio/midi/decoupled ports, external port discovery, dummy controls).
- [ ] Extend `audio_midi_driver_cxx.rs` (or split bridges) to expose the full driver contract needed by backend C++ API wiring.
- [ ] Add/adjust Rust unit tests for new driver lifecycle and metadata methods.
- [ ] Add/adjust bridge-level tests for driver kind metadata, handle lifetimes, and lifecycle edge cases.

- [ ] Introduce Rust-owned driver registry + opaque runtime handle model (parallel path) without removing existing C++ path yet.
- [ ] Migrate `create_audio_driver` / `destroy_audio_driver` to Rust-owned handle path while preserving API behavior.
- [ ] Run milestone verification: `cargo build`, `cargo test`, backend `test_runner`.

- [ ] Move process-cycle orchestration into Rust with exact order parity.
- [ ] Preserve command-queue semantics during orchestration migration (bridge callbacks if queue remains C++ temporarily).
- [ ] Validate RT-thread behavior (no new lock contention on hot path).
- [ ] Validate parity with focused dummy-mode/process tests.

- [ ] Port Dummy backend fully to Rust-native production path.
- [ ] Route all dummy C API endpoints (`dummy_audio_*`, `dummy_driver_*`) through Rust-owned driver path.
- [ ] Remove production dependence on C++ `DummyAudioMidiDriver` class.
- [ ] Run milestone verification: `cargo build`, `cargo test`, backend `test_runner`, `./target/debug/shoopdaloop_dev.sh --self-test`.

- [ ] Implement chosen JACK migration strategy (Rust-native JACK or minimal C++ callback trampoline with Rust-owned state).
- [ ] Preserve and validate JACK behavior/telemetry parity (xrun/sample-rate/buffer-size/dsp-load/port updates).
- [ ] Remove production dependence on C++ JACK inheritance path.

- [ ] Migrate all backend driver API call sites in `libshoopdaloop_backend.cpp` to Rust-handle dispatch end-to-end.
- [ ] Remove C++ `g_active_drivers` ownership model and `AudioMidiDriver` pointer-based runtime handle mapping.
- [ ] Validate backend session wiring (`set_audio_driver`, open port flows, decoupled MIDI flows) on Rust-owned handles.

- [ ] Remove compatibility scaffolding that only supports incremental migration.
- [ ] Delete C++ `AudioMidiDriver` class and class-based production paths.
- [ ] Refactor tests that depended on C++ class internals to stable API/behavior-oriented tests.

- [ ] Run post-removal verification: `cargo build`, `cargo test`, backend `test_runner`, `./target/debug/shoopdaloop_dev.sh --self-test`.
- [ ] Final cleanup: remove dead code/includes, tighten bridge naming/comments, update architecture docs for Rust ownership.
- [ ] Run final quality gates: `cargo fmt --all`, `RUSTFLAGS="-D warnings" cargo build`, `cargo test`, backend `test_runner`, `./target/debug/shoopdaloop_dev.sh --self-test`.
- [ ] Ensure end state: no C++ `AudioMidiDriver` wrapper/class in production path, behavior preserved, all tests passing, no warnings, Rust formatting clean.