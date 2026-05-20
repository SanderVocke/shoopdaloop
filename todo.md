# TODO: Port DummyAudioMidiDriver to Rust

## Phase 1 — Rust Implementation & CXX Bridge

- [x] Create `src/rust/backend_rust/src/dummy_audio_midi_driver.rs`
  - [x] Define `DummyAudioMidiDriver` struct with `AtomicBool`/`AtomicU32` fields
  - [x] Implement `new()` with correct defaults
  - [x] Implement `is_finish()` / `set_finish()`
  - [x] Implement `enter_mode(u32)` (sets mode, resets controlled_samples)
  - [x] Implement `get_mode()`
  - [x] Implement `pause()` / `resume()` / `is_paused()`
  - [x] Implement `controlled_mode_request_samples(u32)` (`fetch_add`)
  - [x] Implement `get_controlled_mode_samples_to_process()`
  - [x] Implement `controlled_mode_advance(u32)` (`fetch_sub`)
  - [x] Add module-level unit tests for all atomic operations
- [x] Create `src/rust/backend_rust/src/dummy_audio_midi_driver_cxx.rs`
  - [x] Define CXX bridge with `extern "Rust"` block
  - [x] Implement thin trampoline functions for each exposed method
- [x] Register modules in `src/rust/backend_rust/src/lib.rs`
- [x] Register bridge in `src/rust/backend_rust/build.rs`
  - [x] Add `"src/dummy_audio_midi_driver_cxx.rs"` to `cxx_build::bridges([...])`
  - [x] Add `rerun-if-changed` line
- [x] Build Rust side and verify no warnings
  ```bash
  cargo build
  ```
- [x] Run Rust tests
  ```bash
  cargo test
  ```

## Phase 2 — C++ Wrapper Refactoring

- [x] Modify `src/backend/internal/DummyAudioMidiDriver.h`
  - [x] Add `#include "backend_rust/src/dummy_audio_midi_driver_cxx.rs.h"`
  - [x] Remove `m_finish`, `m_mode`, `m_controlled_mode_samples_to_process`, `m_paused`
  - [x] Add `rust::Box<backend_rust::DummyAudioMidiDriver> m_rust;`
  - [x] Ensure template declaration and all other members stay unchanged
- [x] Modify `src/backend/internal/DummyAudioMidiDriver.cpp`
  - [x] Update constructor initializer list to initialize `m_rust`
  - [x] Rewrite `enter_mode()` to delegate to `m_rust` and call `wait_process()`
  - [x] Rewrite `get_mode()` to read from `m_rust`
  - [x] Rewrite `controlled_mode_request_samples()` to enqueue command that calls `m_rust`
  - [x] Rewrite `get_controlled_mode_samples_to_process()` to read from `m_rust`
  - [x] Rewrite `start()` thread lambda to query `m_rust->is_finish()`, `m_rust->is_paused()`, `m_rust->get_mode()`, `m_rust->get_controlled_mode_samples_to_process()`, and `m_rust->controlled_mode_advance()`
  - [x] Rewrite `pause()` / `resume()` to delegate to `m_rust`
  - [x] Rewrite `close()` to call `m_rust->set_finish()` before join/detach
  - [x] Rewrite `controlled_mode_run_request()` to poll `m_rust->get_mode()` and `m_rust->get_controlled_mode_samples_to_process()`
  - [x] Verify `open_audio_port()`, `open_midi_port()`, `find_external_ports()`, mock-port helpers require no changes
- [x] Build full project
  ```bash
  cargo build
  ```
- [x] Fix any C++ compilation errors or linking issues
  - Note: build succeeded with expected dead_code warnings on CXX trampolines (C++ uses them through generated bindings, Rust can't see that)
- [x] Fix enum default value bug
  - C++ `DummyAudioMidiDriverMode` has `Controlled=0, Automatic=1`. Initial Rust code defaulted `mode` to `0`, which is `Controlled`. Fixed to default to `1` (`Automatic`) to match original C++ behavior.

## Phase 3 — Testing & Cleanup

- [x] Run C++ unit tests for DummyAudioMidiDriver
  ```bash
  $(find target -name test_runner -type f -executable | head -1) "[DummyAudioMidiDriver]"
  ```
  Result: All 6 test cases passed (14 assertions).
- [x] Run C++ unit tests for DummyPorts
  ```bash
  $(find target -name test_runner -type f -executable | head -1) "[DummyPorts]"
  ```
  Result: No tests matched this tag (there are no `[DummyPorts]` tests in the suite).
- [x] Run all C++ backend tests
  ```bash
  $(find target -name test_runner -type f -executable | head -1)
  ```
  Result: 149/149 passed (5894 assertions).
- [x] Run QML self-tests
  ```bash
  QT_QPA_PLATFORM=offscreen ./target/debug/shoopdaloop_dev.sh --self-test
  ```
  Result: All 186 test cases passed.
- [x] Run Rust tests one final time
  ```bash
  cargo test
  ```
  Result: All passed (86 Rust unit tests + 7 new dummy_audio_midi_driver tests).
- [x] Format all Rust code
  ```bash
  cargo fmt --all
  ```
- [x] Final warning-free build
  ```bash
  RUSTFLAGS="-D warnings" cargo build
  ```
  Result: Build succeeded with no warnings.
- [x] Verify no leftover references to removed C++ member variables (`m_finish`, `m_mode`, `m_controlled_mode_samples_to_process`, `m_paused`)
- [x] Confirm `JackAudioMidiDriver` and `AudioMidiDriver` sources were not modified
