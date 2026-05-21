# Plan: Port CommandQueue from C++ to Rust

## Overview

`CommandQueue` is a lock-free single-producer single-consumer (SPSC) queue for passing functors to be executed in another thread (typically the audio processing thread). It provides a fallback mechanism when the processing thread appears inactive.

**Location in repo:**
- C++ source: `src/backend/internal/CommandQueue.h`, `src/backend/internal/CommandQueue.cpp`
- Rust destination: `src/rust/backend_rust/src/command_queue.rs`, `src/rust/backend_rust/src/command_queue_cxx.rs`

**Files that include CommandQueue (will need updates):**
- `src/backend/internal/AudioChannel.h`
- `src/backend/internal/AudioMidiDriver.h`
- `src/backend/internal/BackendSession.h`
- `src/backend/internal/BasicLoop.h`
- `src/backend/internal/BufferQueue.h`
- `src/backend/internal/DummyAudioPort.h`
- `src/backend/internal/DummyMidiPort.h`
- `src/backend/internal/MidiChannel.h`

## C++ Interface Summary

```cpp
class CommandQueue : private boost::lockfree::spsc_queue<std::function<void()>>,
                     private ModuleLoggingEnabled<"Backend.CommandQueue"> {
    const uint32_t ma_timeout_ms;
    const uint32_t ma_poll_interval_us;
    std::atomic<bool> ma_passthrough_all = false;
    std::atomic<uint64_t> ma_last_processed = 0;

    CommandQueue(uint32_t fixed_size, uint32_t timeout_ms, uint32_t poll_interval_us);
    void queue(std::function<void()> fn);
    void queue_and_wait(std::function<void()> fn);
    void passthrough_on();
    void PROC_exec_all();
    void clear();
    // Convenience methods
    void exec_process_thread_command(std::function<void()> fn) { queue_and_wait(std::move(fn)); }
    void queue_process_thread_command(std::function<void()> fn) { queue(std::move(fn)); }
    void PROC_handle_command_queue() { PROC_exec_all(); }
};
```

## Behavior

1. **queue()**: If processing thread is active (> timeout since last processed), push to lockfree queue with retries. Otherwise execute directly.

2. **queue_and_wait()**: Push command wrapped with completion flag, then spin-wait until finished. Propagates exceptions from command execution.

3. **passthrough_on()**: Sets flag so all commands execute immediately on calling thread.

4. **PROC_exec_all()**: Pops and executes all queued commands, catching exceptions.

5. **clear()**: Resets the queue.

## Challenges

1. **Closure passing**: C++ uses `std::function<void()>`. CXX doesn't support std::function across FFI. Need to use `Box<dyn FnOnce()>` via CXX bridge.

2. **Lock-free queue**: Replace `boost::lockfree::spsc_queue` with `crossbeam_queue::ArrayQueue` (already in dependencies).

3. **Timeout mechanism**: Uses std::chrono for wall-clock timing. Rust equivalent straightforward.

4. **Passthrough mode**: Flag causes immediate execution on calling thread.

5. **Atomic timestamp**: Tracks last processed time to detect inactive processing thread.

## Phases

### Phase 1: Rust Implementation (`command_queue.rs`)

- Create `CommandQueue` struct using `crossbeam_queue::ArrayQueue`
- Implement timestamp tracking for activity detection
- Implement `queue()`, `queue_and_wait()`, `passthrough_on()`, `PROC_exec_all()`, `clear()`
- Add comprehensive unit tests

### Phase 2: CXX Bridge (`command_queue_cxx.rs`)

- Use CXX bridge with `Box<dyn FnOnce()>` for closure passing
- Expose constructor and all methods to C++
- Add bridge file to `build.rs`

### Phase 3: C++ Integration

- Replace C++ files with thin wrapper delegating to Rust via CXX bridge
- Keep same public interface for all including classes

### Phase 4: Testing

- Run `cargo test`
- Build with `cargo build`
- Run C++ tests and QML self-tests

## Build Instructions

```bash
cargo build
```

If final build:
```bash
cargo fmt --all
cargo build RUSTFLAGS="-D warnings"
```

## Test Instructions

- Rust unit tests: `cargo test`
- C++ Catch2 tests: compiled into test_runner from backend build
- QML self-tests: `./target/debug/shoopdaloop_dev.sh --self-test`

## Key Dependencies

- `crossbeam-queue` (workspace, already present)
- `cxx` (workspace, already present)

## Verification Checklist

1. `cargo test` passes for backend_rust
2. C++ tests compile and pass
3. `cargo fmt --check` passes
4. No compiler warnings
5. Application runs without errors