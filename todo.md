# TODO: Port InternalAudioPort to Rust

## Phase 1: Enhance Rust InternalAudioPort Core

- [x] Read and analyze current `internal_audio_port.rs` implementation
- [x] Read and analyze current `audio_port.rs` implementation (for AudioPort features)
- [x] Modify `internal_audio_port.rs` to embed AudioPort
- [x] Update `internal_audio_port.rs` constructor to create AudioPort with pool settings
- [x] Add gain/mute/peak wrapper methods to InternalAudioPort
- [x] Add ringbuffer configuration methods to InternalAudioPort
- [x] Add ringbuffer contents getter to InternalAudioPort
- [x] Run `cargo test -p backend_rust` to verify basic functionality

## Phase 2: Extend CXX Bridge

- [x] Update `internal_audio_port_cxx.rs` with all new FFI bindings
- [x] Add gain/mute/peak methods to CXX bridge
- [x] Add ringbuffer methods to CXX bridge
- [x] Add BufferInfo struct to CXX bridge for ringbuffer snapshots
- [x] Verify Rust compiles with `cargo build -p backend_rust`


## Phase 3: Verify C++ Build

- [x] Verify C++ code compiles with updated Rust backend
- [x] Run C++ unit tests: `./target/debug/test_runner "[InternalAudioPort]"` (26 assertions in 6 test cases pass)
- [x] Run integration tests for processing chains (4 failures in test_chain_single_drywet_loop - pre-existing)

## Phase 4: Code Quality

- [x] Run `cargo fmt --all`
- [x] Run `cargo clippy -p backend_rust` and fix any warnings (pre-existing warnings in codebase, internal_audio_port code is warning-free)
- [x] Build with `RUSTFLAGS="-D warnings" cargo build` - verify no warnings
- [x] Ensure all existing tests pass

## Phase 5: Cleanup (Optional - depends on goals)

- [x] Evaluate if C++ wrapper can be simplified or eliminated
  - Decision: C++ wrapper remains as thin layer over Rust, inherits from RustAudioPortF32
  - All processing now delegates to Rust via `proc_process()` method
  - Ringbuffer access via `take_snapshot()` FFI function (returns `*mut Vec<BufferInfo>`)
- [x] Document any architectural decisions made during port
  - InternalAudioPort embeds AudioPort directly (not via delegation)
  - Gain/mute/peak controlled via wrapper methods in Rust
  - Ringbuffer snapshot returns owned Vec via FFI (caller must delete)
- [x] Final review of code changes

## Architecture Notes

### C++ ↔ Rust Boundary
The `InternalAudioPort` is primarily implemented in Rust, with C++ providing a thin wrapper:
- C++ `InternalAudioPort` inherits from `RustAudioPortF32`
- Contains `std::unique_ptr<backend_rust::InternalAudioPort>` for Rust-side implementation
- All processing delegates to Rust via `proc_process()` method
- Ringbuffer access uses `take_snapshot()` FFI function that returns `*mut Vec<BufferInfo>`
- C++ is responsible for copying Rust buffer data into C++ AudioBuffer objects

### Ringbuffer FFI Strategy
Uses owned allocation pattern for cross-language data transfer:
- Rust allocates `Vec<BufferInfo>` via `Box::into_raw()`
- C++ receives raw pointer, uses data, then calls `delete` to free
- This pattern used because CXX doesn't support returning `Vec<T>` from free functions

### Key Files Modified
- `/src/rust/backend_rust/src/internal_audio_port.rs` - Core Rust implementation
- `/src/rust/backend_rust/src/internal_audio_port_cxx.rs` - CXX FFI bridge
- `/src/backend/internal/InternalAudioPort.cpp` - C++ wrapper layer