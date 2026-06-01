# TODO: make `AudioMidiDriver` a pure interface with composed Rust-backed runtime

- [x] Baseline and orientation
  - [x] Read `AGENTS.md`, `.agents/index.md`, `.agents/build.md`, and `.agents/test.md`
  - [x] Review current `AudioMidiDriver.h/.cpp`, `DummyAudioMidiDriver.h/.cpp`, `JackAudioMidiDriver.h/.cpp`, `AudioMidiDriverCxxTrampolines.*`, and the Rust bridge files under `src/rust/backend_rust/src/`
  - [x] Run baseline `cargo build` if the environment is ready
  - nuance: baseline build attempt reached backend build but failed due environment (`/home/sander/.cargo` registry write is read-only), not project compile errors.

- [x] Phase 1: extract shared behavior into `AudioMidiDriverRuntime`
  - [x] Add `src/backend/internal/AudioMidiDriverRuntime.h`
  - [x] Add `src/backend/internal/AudioMidiDriverRuntime.cpp`
  - [x] Move the Rust `AudioMidiDriverCore` member from `AudioMidiDriver` into `AudioMidiDriverRuntime`
  - [x] Move the Rust command queue member from `AudioMidiDriver` into `AudioMidiDriverRuntime`
  - [x] Move processor weak-pointer storage and processor handle bookkeeping into `AudioMidiDriverRuntime`
  - [x] Move decoupled MIDI registration/keepalive bookkeeping into `AudioMidiDriverRuntime`
  - [x] Move maybe-process callback storage into `AudioMidiDriverRuntime`
  - [x] Implement runtime methods for processor add/remove/list
  - [x] Implement runtime methods for process-cycle dispatch and command execution
  - [x] Implement runtime methods for xruns, sample rate, buffer size, DSP load, active state, client name, client handle, and last-processed state
  - [x] Implement runtime methods for command queue forwarding and `wait_process()`
  - [x] Implement runtime methods for decoupled MIDI port creation from an already-opened `MidiPort` and for decoupled MIDI unregistration
  - [x] Move or keep available the `backend_rust::audiomididriver_*` trampoline functions declared in `AudioMidiDriverCxxTrampolines.h`
  - [x] Update `AudioMidiDriver` so it owns an `AudioMidiDriverRuntime` member and forwards existing behavior to it
  - [x] Keep JACK and Dummy behavior unchanged except for any necessary include or forwarding adjustments
  - [x] Remove obsolete direct state members from `AudioMidiDriver`
  - nuance: trampolines were kept in `AudioMidiDriver.cpp` for link visibility and compatibility with the current Rust bridge.

- [x] Phase 1 validation
  - [x] Run `cargo build`
  - [x] Run `cargo test`
  - [x] Locate backend Catch2 runner with `find target -type f -name test_runner` if needed
  - [x] Run backend `test_runner`
  - [x] Fix all compile errors, test failures, and project warnings introduced by Phase 1
  - nuance: `cargo test` produced pre-existing linker deprecation warnings (`gold linker is deprecated`) in unrelated crates; no new Phase 1 warnings/errors were introduced.

- [x] Phase 2: convert `AudioMidiDriver` to a pure abstract interface
  - [x] Remove `AudioMidiDriverRuntime` ownership from `AudioMidiDriver`
  - [x] Remove shared implementation methods from `AudioMidiDriver.cpp`
  - [x] Change `AudioMidiDriver.h` methods used by callers into pure virtual interface methods
  - [x] Remove implementation-detail setters and process helpers from the public/protected base interface where possible
  - [x] Keep or add only the minimal shared-from-this support needed for `DecoupledMidiPort` weak driver references
  - [x] Ensure `AudioMidiDriver` no longer includes Rust CXX bridge headers unless strictly needed for declarations

- [x] Phase 2: make JACK own and use the runtime
  - [x] Add `AudioMidiDriverRuntime m_runtime` to `GenericJackAudioMidiDriver<API>`
  - [x] Initialize `m_runtime` from the JACK driver constructor's `maybe_process_callback`
  - [x] Replace `AudioMidiDriver::PROC_process(...)` with `m_runtime.process(...)`
  - [x] Replace `AudioMidiDriver::report_xrun()` with `m_runtime.report_xrun()`
  - [x] Replace `AudioMidiDriver::set_*` and `get_*` shared-state calls with `m_runtime` calls
  - [x] Implement all pure `AudioMidiDriver` interface methods in `GenericJackAudioMidiDriver<API>`
  - [x] Preserve JACK-specific refresh behavior for sample rate, buffer size, and DSP load
  - [x] Preserve JACK-specific `wait_process()` behavior when `API::supports_processing` is false
  - [x] Preserve external port discovery behavior

- [x] Phase 2: make Dummy own and use the runtime
  - [x] Add `AudioMidiDriverRuntime m_runtime` to `DummyAudioMidiDriver<Time, Size>`
  - [x] Initialize `m_runtime` from the Dummy driver constructor's `maybe_process_callback`
  - [x] Replace startup state setup calls with `m_runtime` calls
  - [x] Replace shared getters, processor methods, command queue methods, decoupled MIDI methods, and `wait_process()` with runtime forwarding
  - [x] Pass `reinterpret_cast<uintptr_t>(&m_runtime)` to the Rust dummy process thread instead of `reinterpret_cast<uintptr_t>(this)`
  - [x] Update `dummy_audiomididriver_exec_commands` to cast `owner_ptr` to `AudioMidiDriverRuntime*`
  - [x] Update `dummy_audiomididriver_process` to cast `owner_ptr` to `AudioMidiDriverRuntime*`
  - [x] Keep Dummy mode, pause/resume, controlled-sample, mock external port, and concrete port creation behavior intact

- [x] Phase 2: update callers and tests
  - [x] Search for `AudioMidiDriver::` qualified calls and remove uses of former base implementation methods
  - [x] Update `src/backend/test/unit/test_DummyAudioMidiDriver.cpp` to call processor registration through the concrete/virtual interface instead of `AudioMidiDriver::add_processor(...)`
  - [x] Verify `AudioMidiDrivers.cpp` still returns concrete drivers as `shoop_shared_ptr<AudioMidiDriver>`
  - [x] Verify `libshoopdaloop_backend.cpp` still compiles and its driver API calls still target the abstract interface
  - [x] Verify dynamic casts to `_DummyAudioMidiDriver`, `JackAudioMidiDriver`, and `JackTestAudioMidiDriver` still work
  - [x] Clean up stale includes and comments in affected C++ files

- [ ] Phase 2 validation
  - [x] Run `cargo build`
  - [x] Run `cargo test`
  - [x] Run backend `test_runner`
  - [ ] Run `./target/debug/shoopdaloop_dev.sh --self-test`
  - [x] Fix all compile errors, test failures, and project warnings introduced by Phase 2

- [ ] Final cleanup and strict verification
  - [ ] Remove dead code left in `AudioMidiDriver.cpp` or document why any remaining code is needed
  - [ ] Confirm `AudioMidiDriver` has no Rust core, command queue, processor container, decoupled MIDI container, or callback storage members
  - [ ] Confirm both JACK and Dummy own an `AudioMidiDriverRuntime` member
  - [ ] Confirm shared behavior is not duplicated between JACK and Dummy
  - [ ] Run `cargo fmt --all`
  - [ ] Run `RUSTFLAGS="-D warnings" cargo build`
  - [ ] Run `cargo test`
  - [ ] Run backend `test_runner`
  - [ ] Run `./target/debug/shoopdaloop_dev.sh --self-test`
  - [ ] Confirm final state: all tests pass, warnings are fixed, and Rust formatting is applied
