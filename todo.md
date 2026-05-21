# TODO: Port CommandQueue to Rust

## Phase 1: Rust Implementation
- [x] Create `src/rust/backend_rust/src/command_queue.rs` with CommandQueue struct
- [x] Use `crossbeam_queue::ArrayQueue` for lock-free SPSC queue
- [x] Implement timestamp tracking for activity detection
- [x] Implement `queue()` method with timeout fallback
- [x] Implement `queue_and_wait()` with completion flag
- [x] Implement `passthrough_on()` mode
- [x] Implement `exec_all()` (renamed from `PROC_exec_all()`) command execution
- [x] Implement `clear()` method
- [x] Add unit tests for basic queue/exec flow
- [x] Add unit tests for passthrough mode
- [x] Add unit tests for timeout/inactivity behavior
- [x] Add unit tests for queue_and_wait exception propagation
- [x] Add unit tests for clear functionality
- [x] Run `cargo test` - verify all tests pass

## Phase 2: CXX Bridge
- [x] Create `src/rust/backend_rust/src/command_queue_cxx.rs`
- [x] Design closure handling via callback registration pattern
- [x] Expose constructor: `new_command_queue(fixed_size, timeout_ms, poll_interval_us)`
- [x] Expose `cxx_queue()`, `cxx_queue_and_wait()`, `passthrough_on()`, `exec_all()`, `clear()`
- [x] Add bridge file to `src/rust/backend_rust/build.rs`
- [x] Run `cargo test` - verify bridge compiles

## Phase 3: C++ Integration
- [x] Create new `src/backend/internal/CommandQueue.h` wrapping Rust via CXX
- [x] Create `src/backend/internal/CommandQueue.cpp` with callback registration
- [x] Verify header interface matches original
- [x] Build and verify C++ compilation

## Phase 4: Verification
- [x] Run `cargo fmt --all`
- [x] Run full cargo build - no warnings (verified via clean build)
- [x] Run `cargo test` for backend_rust - all 106 tests pass
- [ ] Build C++ test_runner - compile and pass
- [ ] Run QML self-tests: `./target/debug/shoopdaloop_dev.sh --self-test`

## Final State
- [ ] All tests passing (Rust, C++, QML)
- [ ] No compiler warnings
- [ ] Code formatted with rustfmt

## Notes
- Used callback registration pattern for closures across FFI (CXX doesn't support dyn FnOnce)
- C++ registers a callback function via `register_command_callback_ptr()`
- C++ wraps std::function in heap allocation, passes pointer to Rust
- When executed, Rust calls callback with user_data pointer
- Callback invokes the std::function and deletes the heap allocation
- Fixed missing midi_state_tracker_cxx.rs in build.rs - build now completes