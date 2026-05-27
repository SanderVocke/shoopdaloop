# Refactor Plan: Remove C++ `CommandQueue` facade and migrate command queue ownership to Rust

## Goal
Eliminate the C++ wrapper class `src/backend/internal/CommandQueue.h/.cpp` and move command queue implementation and ownership semantics fully into Rust, while keeping C++ callers operational through a CXX bridge API during migration. Preserve behavior of queueing, queue-and-wait, process-thread execution, passthrough mode, inactivity fallback, and queue draining.

This plan also closes existing lifetime/ownership hazards in the current pointer-callback bridge (notably leaked queued C++ commands on `clear()` / drop).

## Where code lives
- C++ facade currently wrapping Rust queue:
  - `src/backend/internal/CommandQueue.h`
  - `src/backend/internal/CommandQueue.cpp`
- Rust queue implementation and CXX bridge:
  - `src/rust/backend_rust/src/command_queue.rs`
  - `src/rust/backend_rust/src/command_queue_cxx.rs`
  - `src/rust/backend_rust/build.rs`
- C++ users of `CommandQueue` (representative, all must compile after migration):
  - `src/backend/internal/AudioMidiDriver.h/.cpp`
  - `src/backend/internal/BackendSession.h/.cpp`
  - `src/backend/internal/AudioChannel.*`, `MidiChannel.*`, `BasicLoop.*`, `BufferQueue.*`, `Dummy*Port.*`
- Global backend glue where includes/types may need update:
  - `src/backend/libshoopdaloop_backend.cpp`

## Design decisions to implement
1. Keep one Rust queue type as source of truth.
2. Remove global static callback registration mechanism (`COMMAND_CALLBACK`) and raw callback pointer API.
3. Replace the single execute-callback global with explicit execute/destroy C++ callbacks, using a `usize user_data` handle transport that remains `Send` across Rust queue threads.
4. Ensure all enqueued C++ commands are released exactly once even if never executed (clear/drop/shutdown).
5. Keep queue thread semantics unchanged: process-thread execution when active; direct fallback when inactive; passthrough option.
6. Treat failures as fatal where they already were fatal; do not prioritize exception translation.

## Phase 1: Introduce safe CXX handle API in Rust bridge
Implement bridge contract in `command_queue_cxx.rs`:
- Expose two C++ functions Rust can call:
  - `unsafe fn cxx_command_execute_ptr(user_data: usize);`
  - `unsafe fn cxx_command_destroy_ptr(user_data: usize);`
- Expose Rust queue methods using `usize` handles:
  - `unsafe fn cxx_queue(self: &CommandQueue, user_data: usize);`
  - `unsafe fn cxx_queue_and_wait(self: &CommandQueue, user_data: usize);`
  - `unsafe fn cxx_exec_process_thread_command(self: &CommandQueue, user_data: usize);`

Rust implementation updates in `command_queue.rs`:
- Add internal command wrapper with `user_data` and `executed` flag.
- On execution path, call `cxx_command_execute_ptr(user_data)` exactly once.
- On non-execution drop path (e.g. `clear`, queue drop), call `cxx_command_destroy_ptr(user_data)` via `Drop`.
- Remove dependence on `COMMAND_CALLBACK`/registration.

## Phase 2: Implement C++ token type and executors
In C++ backend internal layer:
- Add a new token class/struct in a dedicated file (preferred):
  - `src/backend/internal/CommandToken.h`
  - `src/backend/internal/CommandToken.cpp`
- Token owns `std::function<void()>` and an executed flag.
- `cxx_command_execute(UniquePtr<CxxCommandToken>)`:
  - Invoke stored functor if present.
  - Mark executed to prevent double action.
  - Release resources.
- `cxx_command_destroy(UniquePtr<CxxCommandToken>)`:
  - Drop functor without invoking.
- Guarantee no exception crosses FFI boundary; if invocation throws, terminate (`std::terminate`) or catch-and-abort.

## Phase 3: Migrate C++ facade internals to token API
Update `src/backend/internal/CommandQueue.cpp`:
- Replace raw `new std::function<void()>` + `usize` conversion with creation of `UniquePtr<CxxCommandToken>`.
- Replace calls:
  - `cxx_queue(...)` -> `cxx_queue_token(...)`
  - `cxx_queue_and_wait(...)` -> `cxx_queue_and_wait_token(...)`
- Remove static callback registrar and `register_command_callback_ptr` usage.

At this point behavior remains same for all callers, but ownership/leak hazards are fixed.

## Phase 4: Remove pointer callback bridge from Rust
After Phase 3 compiles/tests pass:
- Delete from Rust:
  - `COMMAND_CALLBACK` static and `set_command_callback`.
  - `register_command_callback_ptr` bridge function.
  - `cxx_queue`, `cxx_queue_and_wait`, `cxx_exec_process_thread_command` pointer variants.
- Remove related comments/docs mentioning callback registration.
- Ensure no remaining callsites in C++.

## Phase 5: Remove C++ `CommandQueue` facade class and use generated bridge type directly
This is the actual facade removal step.

### 5.1 Replace member types and includes
Across C++ classes currently holding `CommandQueue m_command_queue;`:
- Replace with `rust::Box<backend_rust::CommandQueue> m_command_queue;` (or project-preferred owning wrapper).
- Include generated bridge header directly where needed (`backend_rust/src/command_queue_cxx.rs.h`) and token helpers.

### 5.2 Replace constructor initialization
- Convert initializer-list construction from `m_command_queue(fixed, timeout, poll)` to `m_command_queue(backend_rust::new_command_queue(...))`.

### 5.3 Replace method calls
Map all facade methods to bridge methods:
- `queue(fn)` -> helper that wraps `fn` into token then `cxx_queue_token`
- `queue_and_wait(fn)` / `exec_process_thread_command(fn)` -> `cxx_queue_and_wait_token`
- `queue_process_thread_command(fn)` -> queue token equivalent
- `PROC_exec_all()` / `PROC_handle_command_queue()` -> `exec_all()`
- `passthrough_on()` -> `passthrough_on()`
- `clear()` -> `clear()`

To avoid repeating wrapping logic in all callsites, create one shared C++ helper function (e.g. in `CommandToken.h`) that takes `rust::Box<backend_rust::CommandQueue>&` and `std::function<void()>`.

### 5.4 Delete facade files and references
- Remove:
  - `src/backend/internal/CommandQueue.h`
  - `src/backend/internal/CommandQueue.cpp`
- Update includes and build references accordingly.

## Phase 6: Shutdown/clear semantics validation and hardening
Add explicit tests (Rust and/or C++ integration) covering:
1. Enqueue from C++, call `clear()`, verify no leak and no execution.
2. Enqueue multiple commands, drop queue without `exec_all`, verify cleanup.
3. Queue-and-wait under active process thread simulation.
4. Inactivity fallback executes inline as before.
5. Passthrough mode executes immediately.

Prefer adding Rust-side unit tests around token wrapper drop behavior and at least one C++ integration test through real bridge.

## Phase 7: Cleanup and documentation
- Update comments in `command_queue.rs` and bridge file to remove stale SPSC/raw-callback wording.
- Ensure terminology matches actual implementation (`ArrayQueue` behavior and thread model).
- Document ownership model: enqueue transfers command ownership to Rust; command is either executed exactly once or destroyed exactly once.

## Build & test instructions for execution
From repo root:
1. `cargo fmt --all`
2. `cargo build`
3. `cargo test`
4. Run backend C++ tests via built `test_runner` produced by build of backend crate.
5. Run QML self-tests:
   - `./target/debug/shoopdaloop_dev.sh --self-test`
6. Final verification build with warnings as errors:
   - `RUSTFLAGS="-D warnings" cargo build`

If dependency/toolchain failures originate outside this repository, report environment issue and stop.

## Expected end state
- No C++ `CommandQueue` class remains.
- C++ users call Rust queue via CXX bridge directly (with shared token helper).
- No global callback registration in Rust queue.
- No raw `usize` user-data path.
- Pending C++ commands are always cleaned up on execute, clear, and drop.
- Project builds/tests pass and formatting is clean.