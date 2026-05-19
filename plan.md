# Refactor: WithCommandQueue mixin → composition

## Goal

Replace `WithCommandQueue` inheritance with a held `CommandQueue` member in all consuming classes.
`WithCommandQueue` is a thin wrapper (4 convenience methods) masquerading as a base class.
Composition eliminates a meaningless inheritance layer and sets the stage for clean Rust porting
(where the command queue is just a struct field, not a base trait).

## Scope

7 classes currently inherit `WithCommandQueue`:

| Class | Inheritance | Active method usage | Complexity |
|---|---|---|---|
| `AudioMidiDriver` | `public` | `PROC_handle_command_queue`, `exec_process_thread_command`, `queue_process_thread_command` | Medium |
| `BackendSession` | `public` | All three methods, plus direct `ma_queue.passthrough_on()` | Medium |
| `BasicLoop` | `protected` | `PROC_handle_command_queue`, `exec_process_thread_command` | Medium |
| `AudioChannel` | `private` | `PROC_handle_command_queue`, `exec_process_thread_command`, `queue_process_thread_command` | Medium |
| `MidiChannel` | `private` | `PROC_handle_command_queue`, `exec_process_thread_command`, `queue_process_thread_command` | Medium |
| `BufferQueue<SampleT>` | `private` | `WithCommandQueue::queue_process_thread_command`, `WithCommandQueue::PROC_handle_command_queue` | Low (2 calls) |
| `DummyAudioMidiDriver` | indirect (via `AudioMidiDriver`) | `exec_process_thread_command`, direct `ma_queue.PROC_exec_all()` | Low |

## Plan

### Step 1: Add convenience helpers to `CommandQueue` (optional but clean)

Add two inline convenience methods to `CommandQueue` to replace the wrapper methods:

```cpp
// In CommandQueue.h:
void exec_process_thread_command(std::function<void()> fn) { queue_and_wait(std::move(fn)); }
void queue_process_thread_command(std::function<void()> fn) { queue(std::move(fn)); }
```

This means all call sites stay identical — only the prefix changes from implicit `this->` to `m_command_queue.`.

### Step 2: Remove `WithCommandQueue` inheritance, add `CommandQueue` member

For each class (in order of increasing dependency complexity):

1. **`BufferQueue`** — simplest, private inheritance, 2 call sites
2. **`AudioChannel`** — private inheritance
3. **`MidiChannel`** — private inheritance
4. **`BasicLoop`** — protected inheritance (note: `AudioMidiLoop` inherits from `BasicLoop` but doesn't use the command queue directly)
5. **`AudioMidiDriver`** — public inheritance, `DecoupledMidiPort` uses it indirectly
6. **`DummyAudioMidiDriver`** — uses inherited methods + direct `ma_queue` access
7. **`BackendSession`** — public inheritance, uses `ma_queue.passthrough_on()` directly

**Per-class changes:**

- Remove `WithCommandQueue` from the inheritance list
- Remove `#include "WithCommandQueue.h"` (keep `#include "CommandQueue.h"`)
- Add `CommandQueue m_command_queue;` member
- Update constructor initializer list to initialize `m_command_queue` with the same args previously passed to `WithCommandQueue(...)`
- Replace all calls:
  - `exec_process_thread_command(fn)` → `m_command_queue.exec_process_thread_command(fn)`
  - `queue_process_thread_command(fn)` → `m_command_queue.queue_process_thread_command(fn)`
  - `PROC_handle_command_queue()` → `m_command_queue.PROC_handle_command_queue()`
  - `ma_queue.xxx()` → `m_command_queue.xxx()`

### Step 3: Delete `WithCommandQueue.h`

After all consumers are migrated, remove the file entirely.

### Step 4: Build & test

## Build

```bash
cargo build
```

The C++ backend is built as a side effect of building the `backend` crate via cxx/cmake.

For a final clean build:

```bash
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

## Test

### C++ unit & integration tests

The test runner is built as a side effect of `cargo build`. Find and run it:

```bash
# Find the test_runner binary
find target -name "test_runner" -type f -executable
# Run it
./target/<profile>/test_runner
# Or with filters for affected areas:
./target/<profile>/test_runner "[DummyPorts]"
./target/<profile>/test_runner "[DummyAudioMidiDriver]"
./target/<profile>/test_runner "[BasicLoop]"
```

### Rust tests

```bash
cargo test -p backend_rust
```

### Full application self-test

```bash
./target/debug/shoopdaloop_dev.sh --self-test
```

## Detailed per-class change list

### 2a. BufferQueue (`BufferQueue.h`, `BufferQueue.cpp`)

- Remove `private WithCommandQueue` from inheritance
- Replace `#include "WithCommandQueue.h"` → `#include "CommandQueue.h"`
- Add `CommandQueue m_command_queue;` member (template class, so inline)
- Constructor: init `m_command_queue` with same args as current `WithCommandQueue()` default
- `BufferQueue.cpp:72`: `WithCommandQueue::queue_process_thread_command(...)` → `m_command_queue.queue_process_thread_command(...)`
- `BufferQueue.cpp:99`: `WithCommandQueue::PROC_handle_command_queue()` → `m_command_queue.PROC_handle_command_queue()`

### 2b. AudioChannel (`AudioChannel.h`, `AudioChannel.cpp`)

- Remove `private WithCommandQueue` from inheritance
- Replace `#include "WithCommandQueue.h"` → `#include "CommandQueue.h"`
- Add `CommandQueue m_command_queue;` member
- `AudioChannel.cpp:156`: Replace `WithCommandQueue(50)` in initializer → `m_command_queue(50, 1000, 1000)` (use WithCommandQueue defaults)
- `AudioChannel.cpp:225`: `PROC_handle_command_queue()` → `m_command_queue.PROC_handle_command_queue()`
- `AudioChannel.cpp:431`: `queue_process_thread_command(fn)` → `m_command_queue.queue_process_thread_command(fn)`
- `AudioChannel.cpp:465`: `exec_process_thread_command(cmd)` → `m_command_queue.exec_process_thread_command(cmd)`
- `AudioChannel.cpp:481`: same as above

### 2c. MidiChannel (`MidiChannel.h`, `MidiChannel.cpp`)

- Remove `private WithCommandQueue` from inheritance
- Replace `#include "WithCommandQueue.h"` → `#include "CommandQueue.h"`
- Add `CommandQueue m_command_queue;` member
- `MidiChannel.cpp:39`: Replace `WithCommandQueue(50)` → `m_command_queue(50, 1000, 1000)`
- `MidiChannel.cpp:132`: `PROC_handle_command_queue()` → `m_command_queue.PROC_handle_command_queue()`
- `MidiChannel.cpp:373`: `exec_process_thread_command(fn)` → `m_command_queue.exec_process_thread_command(fn)`
- `MidiChannel.cpp:575`: same
- `MidiChannel.cpp:624`: same
- `MidiChannel.cpp:733`: `queue_process_thread_command(fn)` → `m_command_queue.queue_process_thread_command(fn)`

### 2d. BasicLoop (`BasicLoop.h`, `BasicLoop.cpp`)

- Remove `protected WithCommandQueue` from inheritance
- Replace `#include "WithCommandQueue.h"` → `#include "CommandQueue.h"`
- Add `protected: CommandQueue m_command_queue;` member (`protected` so `AudioMidiLoop` can access it)
- `BasicLoop.cpp:18`: Replace `WithCommandQueue(100)` → `m_command_queue(100, 1000, 1000)`
- `BasicLoop.cpp:150`: `PROC_handle_command_queue()` → `m_command_queue.PROC_handle_command_queue()`
- `BasicLoop.cpp:203, 287, 302, 318, 333, 390, 409, 449, 460`: `exec_process_thread_command(fn)` → `m_command_queue.exec_process_thread_command(fn)`

### 2d-ii. AudioMidiLoop (`AudioMidiLoop.h`, `AudioMidiLoop.cpp`)

`AudioMidiLoop` inherits from `BasicLoop` (which uses `protected` inheritance for the queue). After the refactor, it accesses the `protected` member directly:

- Remove `#include "WithCommandQueue.h"` from `AudioMidiLoop.cpp`
- `AudioMidiLoop.cpp:26,45,57,77,94,105,120,133`: `exec_process_thread_command(fn)` → `this->m_command_queue.exec_process_thread_command(fn)`

### 2e. AudioMidiDriver (`AudioMidiDriver.h`, `AudioMidiDriver.cpp`)

- Remove `public WithCommandQueue` from inheritance
- Replace `#include "WithCommandQueue.h"` → `#include "CommandQueue.h"`
- Add `CommandQueue m_command_queue;` member (protected, since `DummyAudioMidiDriver` may need access)
- `AudioMidiDriver.cpp:8`: Replace `WithCommandQueue()` → `m_command_queue(shoop_constants::command_queue_size, 1000, 1000)`
- `AudioMidiDriver.cpp:44`: `PROC_handle_command_queue()` → `m_command_queue.PROC_handle_command_queue()`
- `AudioMidiDriver.cpp:65`: `exec_process_thread_command(...)` → `m_command_queue.exec_process_thread_command(...)`
- `AudioMidiDriver.cpp:144,146`: same
- `AudioMidiDriver.cpp:158`: `queue_process_thread_command(...)` → `m_command_queue.queue_process_thread_command(...)`

### 2f. DummyAudioMidiDriver (`DummyAudioMidiDriver.h`, `DummyAudioMidiDriver.cpp`)

- This class inherits from `AudioMidiDriver` (which previously inherited `WithCommandQueue`), so it
  got `exec_process_thread_command` etc. for free. Now it must use the member.
- Remove `#include "WithCommandQueue.h"` (no longer needed)
- `DummyAudioMidiDriver.cpp:46`: `exec_process_thread_command(...)` → `this->m_command_queue.exec_process_thread_command(...)`
- `DummyAudioMidiDriver.cpp:94`: `ma_queue.PROC_exec_all()` → `this->m_command_queue.PROC_exec_all()`
- `DummyAudioMidiDriver.cpp:104`: `PROC_handle_command_queue()` → `this->m_command_queue.PROC_handle_command_queue()`

### 2g. BackendSession (`BackendSession.h`, `BackendSession.cpp`)

- Remove `public WithCommandQueue` from inheritance
- Replace `#include "WithCommandQueue.h"` → `#include "CommandQueue.h"`
- Add `CommandQueue m_command_queue;` member
- `BackendSession.cpp:104`: Replace `WithCommandQueue()` → `m_command_queue(shoop_constants::command_queue_size, 1000, 1000)`
- `BackendSession.cpp:142`: `PROC_handle_command_queue()` → `m_command_queue.PROC_handle_command_queue()`
- `BackendSession.cpp:198`: `ma_queue.passthrough_on()` → `m_command_queue.passthrough_on()`
- `BackendSession.cpp:444`: `exec_process_thread_command(...)` → `m_command_queue.exec_process_thread_command(...)`
- `BackendSession.cpp:607`: `queue_process_thread_command(...)` → `m_command_queue.queue_process_thread_command(...)`

### Step 3: Delete `WithCommandQueue.h`

```bash
rm src/backend/internal/WithCommandQueue.h
```
