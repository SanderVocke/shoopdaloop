# Refactor Plan: Full migration from C++ `CommandQueue` facade to direct Rust queue ownership

## Goal
Remove `src/backend/internal/CommandQueue.h/.cpp` completely and make all backend C++ classes use `backend_rust::CommandQueue` through a shared helper API. Preserve behavior and fix ownership/lifetime guarantees for queued C++ commands.

## Current status (already completed)
- Rust/C++ bridge now uses explicit execute/destroy callbacks with `usize user_data` handles:
  - Rust calls C++ `cxx_command_execute_ptr` / `cxx_command_destroy_ptr`.
  - Rust wrapper ensures exactly-once execute-or-destroy semantics even on `clear()`/drop.
- Static global callback registration path is removed from active flow.
- Added C++ command handle helpers:
  - `src/backend/internal/CommandToken.h/.cpp`
- Added shared direct helper API:
  - `src/backend/internal/RustCommandQueue.h`
- Header visibility issues between cxxbridge compile and backend CMake compile resolved.
- Baseline validations pass: `cargo build`, `cargo test`, backend `test_runner`.

## Key migration insight
Directly converting base classes like `AudioMidiDriver` first causes broad breakage in template-derived classes (`DummyAudioMidiDriver`, `JackAudioMidiDriver`, etc.) because many callsites currently assume facade methods (`PROC_exec_all`, `exec_process_thread_command`, etc.).

Therefore full migration should proceed by introducing and using shared helper calls incrementally, then swapping member types, then deleting facade.

## Final target API shape
- Queue ownership in C++ classes:
  - `rust::Box<backend_rust::CommandQueue> m_command_queue;`
- Queue operations always via helper functions in `RustCommandQueue.h`:
  - `make(...)`, `queue(...)`, `queue_and_wait(...)`, `exec_all(...)`, `passthrough_on(...)`, `clear(...)`
- No class should call facade-style methods (`PROC_exec_all`, `queue_process_thread_command`, etc.) on queue objects.

## Detailed staged execution

### Stage A: Stabilize helper-first call pattern everywhere
1. Keep existing `CommandQueue` facade in place temporarily.
2. Replace direct queue method invocations across backend code with helper-style calls wherever feasible, especially in template-heavy files:
   - `DummyAudioMidiDriver.cpp`
   - `jack/JackAudioMidiDriver.cpp`
   - `AudioChannel.cpp`, `MidiChannel.cpp`, `BasicLoop.cpp`, `BufferQueue.cpp`
   - `DummyAudioPort.cpp`, `DummyMidiPort.cpp`, etc.
3. For API glue expecting `CommandQueue&` (e.g. `evaluate_before_or_after_process`), support both temporary types during transition (already started).
4. Build/test after each subgroup migration.

### Stage B: Migrate non-template core owners fully
1. Migrate `BackendSession` and `AudioMidiDriver` members and forwarding methods to direct Rust box type.
2. Update getters return type to `backend_rust::CommandQueue&`.
3. Update constructor initialization to `rust_command_queue::make(...)`.
4. Ensure all member operations route through helper wrappers.

### Stage C: Migrate remaining owners and utilities
1. Migrate all classes with `CommandQueue m_command_queue` to Rust box ownership:
   - `AudioChannel`, `MidiChannel`, `BasicLoop`, `BufferQueue`, `DummyAudioPort`, `DummyMidiPort`, etc.
2. Replace includes from `CommandQueue.h` to `RustCommandQueue.h` (and bridge headers as needed).
3. Replace any lingering facade-style methods with helper calls.

### Stage D: Delete facade and clean references
1. Remove all includes/usages of `CommandQueue.h` from codebase.
2. Delete:
   - `src/backend/internal/CommandQueue.h`
   - `src/backend/internal/CommandQueue.cpp`
3. Update CMake/source lists as needed.
4. Ensure no symbol/type dependency remains on `CommandQueue` class.

### Stage E: Tests for ownership/lifetime semantics
Add/adjust tests to confirm behavior remains correct after full migration:
1. Clearing queue destroys pending C++ command allocations without executing them.
2. Queue destruction with pending commands destroys commands.
3. queue-and-wait completion and fallback behavior remain unchanged.
4. passthrough mode and inactivity logic unchanged from caller perspective.

### Stage F: Documentation cleanup
1. Update comments in Rust queue and bridge files for current handle-based model.
2. Remove stale wording about old callback registration.
3. Document final ownership model clearly.

## Build/test checkpoints
At each stage milestone run:
1. `cargo build`
2. `cargo test`
3. backend Catch2 test runner:
   - `target/debug/build/backend-*/out/cmake_build/build/test/test_runner` (resolved built path)

Final gate:
1. `cargo fmt --all`
2. `RUSTFLAGS="-D warnings" cargo build`
3. `cargo test`
4. backend `test_runner`
5. `./target/debug/shoopdaloop_dev.sh --self-test` (if it fails due to known environment crash-handling startup issue, report as environment/runtime blocker with logs).

## Expected end state
- `CommandQueue.h/.cpp` removed.
- All backend users own/use `backend_rust::CommandQueue` directly.
- Unified helper API used for queue operations from C++.
- Exactly-once execute-or-destroy semantics maintained for queued C++ commands.
- All available builds/tests pass; final warnings gate passes.