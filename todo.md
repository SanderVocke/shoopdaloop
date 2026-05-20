# TODO: Port InternalAudioPort and InternalMidiPort

## Phase 1: InternalAudioPort Rust Core
- [x] Create `src/rust/backend_rust/src/internal_audio_port.rs` with `InternalAudioPort` struct, buffer management, and unit tests
- [x] Create `src/rust/backend_rust/src/internal_audio_port_cxx.rs` with CXX bridge exposing constructor, name, proc_prepare, proc_get_buffer, connectability
- [x] Update `src/rust/backend_rust/src/lib.rs` to add the two new modules
- [x] Update `src/rust/backend_rust/build.rs` to include `src/internal_audio_port_cxx.rs` in the bridges array and add rerun-if-changed
- [x] `cargo build` passes with no errors
- [x] `cargo test` passes (new Rust unit tests for InternalAudioPort)

## Phase 1: InternalAudioPort C++ Wrapper
- [x] Modify `src/backend/internal/InternalAudioPort.h` to replace direct members with `rust::Box<backend_rust::InternalAudioPort>` and keep all virtual overrides
- [x] Modify `src/backend/internal/InternalAudioPort.cpp` so constructor initializes Rust core and all methods delegate to it; PROC_process calls base AudioPort process on Rust buffer
- [x] `cargo build` passes (C++ wrapper compiles)
- [x] C++ `test_runner` builds and existing `test_InternalAudioPort.cpp` tests pass

## Phase 2: InternalMidiPort Rust Core
- [x] Create `src/rust/backend_rust/src/internal_midi_port.rs` with `InternalMidiPort` struct, event buffer, and unit tests
- [x] Create `src/rust/backend_rust/src/internal_midi_port_cxx.rs` with CXX bridge exposing constructor, name, proc_prepare, n_events, get_event_time/size/bytes, write_event, connectability
- [x] Update `src/rust/backend_rust/src/lib.rs` to add the two new modules
- [x] Update `src/rust/backend_rust/build.rs` to include `src/internal_midi_port_cxx.rs` in the bridges array and add rerun-if-changed
- [x] `cargo build` passes with no errors
- [x] `cargo test` passes (new Rust unit tests for InternalMidiPort)

## Phase 2: InternalMidiPort C++ Wrapper
- [x] Create `src/backend/internal/InternalMidiPort.h` declaring class that inherits `MidiPort`, `MidiReadableBuffer`, `MidiWriteableBuffer`, with a `rust::Box<backend_rust::InternalMidiPort>` member
- [x] Create `src/backend/internal/InternalMidiPort.cpp` implementing all delegation; PROC_prepare clears Rust events then calls MidiPort::PROC_prepare; PROC_process calls MidiPort::PROC_process; buffer accessors return `this`
- [x] `cargo build` passes (C++ wrapper compiles)
- [x] C++ `test_runner` builds and existing MIDI integration tests still pass

## Phase 2: C API Integration
- [x] Add `#include "InternalMidiPort.h"` to `src/backend/libshoopdaloop_backend.cpp`
- [x] Replace the `throw std::runtime_error` in `open_internal_midi_port` with construction of `InternalMidiPort`, addition to backend via `add_midi_port`, and graph change notification
- [x] `cargo build` passes
- [x] C++ `test_runner` all tests pass
- [x] Optional: create `src/backend/test/unit/test_InternalMidiPort.cpp` with basic property, write/read, and mute tests. Also added the file to `src/backend/test/CMakeLists.txt` because test sources are explicitly listed there, not globbed.

## Phase 3: Cleanup and Final Verification
- [x] Remove any dead or commented-out C++ code in `InternalAudioPort.cpp` and `InternalMidiPort.cpp`
- [x] Run `cargo fmt --all`
- [x] Run `RUSTFLAGS="-D warnings" cargo build` — must pass with zero warnings
- [x] Run `cargo test` — must pass
- [x] Run C++ `test_runner` — all tests must pass (5910 assertions in 153 test cases)
- [x] If applicable, run QML self-tests with `QT_QPA_PLATFORM=offscreen ./target/debug/shoopdaloop_dev.sh --self-test` — **186 testcases passed, 0 failed**
- [x] Final review: original C++ functionality fully replaced by Rust core, thin wrappers only, no regressions
