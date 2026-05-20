# Port Plan: DummyAudioMidiDriver (C++ → Rust)

## Goal
Move the implementation of `DummyAudioMidiDriver` from C++ to Rust while preserving its public C++ API, its inheritance from `AudioMidiDriver`, and its role in tests and the C API layer. The C++ template class `DummyAudioMidiDriver<Time, Size>` remains as a thin wrapper; all mutable driver state and the business logic for controlled/automatic mode moves to Rust.

This class was chosen because it is a leaf driver class (no other production code inherits from it) and because it exercises the port→driver boundary without entangling the JACK driver or the graph processing subsystem.

---

## Architecture & Constraints

### Why keep a C++ wrapper?
`DummyAudioMidiDriver` inherits from `AudioMidiDriver`. Several parts of the codebase depend on this inheritance and on the exact C++ type identity:

- `libshoopdaloop_backend.cpp` does `dynamic_cast<_DummyAudioMidiDriver*>` and `shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>`.
- `AudioMidiDrivers.cpp` constructs it as `shoop_make_shared<shoop_types::_DummyAudioMidiDriver>`.
- Integration tests cast the generic driver to `_DummyAudioMidiDriver` to access dummy-specific methods.
- `JackAudioMidiDriver` (not being ported now) also inherits from `AudioMidiDriver`. We must not destabilize that hierarchy.

Therefore we keep `DummyAudioMidiDriver<Time, Size>` as a C++ template class that inherits from `AudioMidiDriver`, but replace its mutable-state member variables with a `rust::Box<backend_rust::DummyAudioMidiDriver>` and delegate all state changes to Rust.

### Why keep the process thread in C++?
The thread loop must call `AudioMidiDriver::PROC_process(uint32_t)`, which lives in the C++ base class and accesses C++-only members (`m_processors`, `m_command_queue`, `m_decoupled_midi_ports`, etc.). Moving the thread to Rust would require a Rust→C++ callback for every process cycle, which is unnecessary complexity and would set an awkward precedent for `JackAudioMidiDriver` later. Instead, the C++ wrapper keeps the `std::thread` spawn and the loop body, but reads all mode/pause/finish/sample-request state from the Rust core via the CXX bridge.

### What moves to Rust?
- `m_finish` → Rust `AtomicBool`
- `m_mode` → Rust `AtomicU32` (0 = Controlled, 1 = Automatic)
- `m_controlled_mode_samples_to_process` → Rust `AtomicU32`
- `m_paused` → Rust `AtomicBool`
- All direct getters/setters for the above
- The `controlled_mode_advance` logic (atomic subtraction)

### What stays in C++?
- Template shell, inheritance, virtual overrides
- `std::thread` spawn/join
- `open_audio_port` / `open_midi_port` (return C++ port types)
- `m_audio_ports`, `m_midi_ports` registries
- `m_external_connections` (already a thin C++ wrapper around Rust)
- `DummyAudioMidiDriverSettings` and `DummyAudioMidiDriverMode` enums
- `wait_process()` inherited from `AudioMidiDriver`

---

## Phase 1: Rust Implementation & CXX Bridge

### 1.1 Create `src/rust/backend_rust/src/dummy_audio_midi_driver.rs`

Implement a `DummyAudioMidiDriver` struct with the following shape:

```rust
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct DummyAudioMidiDriver {
    finish: AtomicBool,
    mode: AtomicU32,        // 0 = Controlled, 1 = Automatic
    controlled_samples: AtomicU32,
    paused: AtomicBool,
}
```

Required methods (all must be thread-safe via `Ordering::SeqCst`):

| Method | Behavior |
|--------|----------|
| `new() -> Self` | Initialize atomics to defaults: `finish = false`, `mode = 1` (Automatic), `controlled_samples = 0`, `paused = false` |
| `is_finish(&self) -> bool` | Read `finish` |
| `set_finish(&self)` | Set `finish = true` |
| `enter_mode(&self, mode: u32)` | Atomically store `mode` and reset `controlled_samples = 0` |
| `get_mode(&self) -> u32` | Read `mode` |
| `pause(&self)` | Set `paused = true` |
| `resume(&self)` | Set `paused = false` |
| `is_paused(&self) -> bool` | Read `paused` |
| `controlled_mode_request_samples(&self, samples: u32)` | `fetch_add` on `controlled_samples` |
| `get_controlled_mode_samples_to_process(&self) -> u32` | Read `controlled_samples` |
| `controlled_mode_advance(&self, samples: u32)` | `fetch_sub` on `controlled_samples` (saturates at 0) |

Note on `controlled_mode_advance`: use `fetch_sub` and if the result underflows, clamp to 0. The original C++ does `m_controlled_mode_samples_to_process -= to_process` without clamping, but since `to_process` is never larger than the current value in practice, a simple `fetch_sub` is sufficient.

### 1.2 Create `src/rust/backend_rust/src/dummy_audio_midi_driver_cxx.rs`

Add a CXX bridge that exposes the struct and methods to C++ under namespace `backend_rust`:

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type DummyAudioMidiDriver;
        fn new_dummy_audio_midi_driver() -> Box<DummyAudioMidiDriver>;
        fn is_finish(self: &DummyAudioMidiDriver) -> bool;
        fn set_finish(self: &DummyAudioMidiDriver);
        fn enter_mode(self: &DummyAudioMidiDriver, mode: u32);
        fn get_mode(self: &DummyAudioMidiDriver) -> u32;
        fn pause(self: &DummyAudioMidiDriver);
        fn resume(self: &DummyAudioMidiDriver);
        fn is_paused(self: &DummyAudioMidiDriver) -> bool;
        fn controlled_mode_request_samples(self: &DummyAudioMidiDriver, samples: u32);
        fn get_controlled_mode_samples_to_process(self: &DummyAudioMidiDriver) -> u32;
        fn controlled_mode_advance(self: &DummyAudioMidiDriver, samples: u32);
    }
}
```

Implement the thin trampoline functions that delegate to the Rust struct methods.

### 1.3 Register new files in Rust crate

- Add `pub mod dummy_audio_midi_driver;` and `pub mod dummy_audio_midi_driver_cxx;` to `src/rust/backend_rust/src/lib.rs`.
- Add `"src/dummy_audio_midi_driver_cxx.rs"` to the `cxx_build::bridges([...])` array in `src/rust/backend_rust/build.rs`.
- Add `println!("cargo:rerun-if-changed=src/dummy_audio_midi_driver_cxx.rs");` in `build.rs`.

---

## Phase 2: Refactor C++ Wrapper

### 2.1 Modify `src/backend/internal/DummyAudioMidiDriver.h`

Replace the four mutable-state member variables:

```cpp
// REMOVE these:
// std::atomic<bool> m_finish = false;
// std::atomic<DummyAudioMidiDriverMode> m_mode = DummyAudioMidiDriverMode::Automatic;
// std::atomic<uint32_t> m_controlled_mode_samples_to_process = 0;
// std::atomic<bool> m_paused = false;

// ADD this:
rust::Box<backend_rust::DummyAudioMidiDriver> m_rust;
```

Keep everything else exactly as-is: the template parameters (which are unused but required for existing instantiations), `m_proc_thread`, `m_audio_ports`, `m_midi_ports`, `m_client_name_str`, callbacks, `m_external_connections`, and all method signatures.

Add the necessary `#include` for the generated CXX header:
```cpp
#include "backend_rust/src/dummy_audio_midi_driver_cxx.rs.h"
```

### 2.2 Modify `src/backend/internal/DummyAudioMidiDriver.cpp`

Rewrite the implementation so that every access to the old member variables goes through `m_rust`. The thread loop must query Rust for state each iteration.

Key rewrites:

**Constructor**
Replace the initializer-list entries for the four removed members with:
```cpp
m_rust(backend_rust::new_dummy_audio_midi_driver()),
```

**`enter_mode`**
```cpp
if ((DummyAudioMidiDriverMode)m_rust->get_mode() != mode) {
    Log::log<log_level_debug>("DummyAudioMidiDriver: mode -> {}", mode_names.at(mode));
    m_rust->enter_mode((uint32_t)mode);
    wait_process();
}
```

**`get_mode`**
```cpp
return (DummyAudioMidiDriverMode)m_rust->get_mode();
```

**`controlled_mode_request_samples`**
```cpp
this->m_command_queue.exec_process_thread_command([this, samples]() {
    m_rust->controlled_mode_request_samples(samples);
    uint32_t requested = m_rust->get_controlled_mode_samples_to_process();
    Log::log<log_level_debug>("DummyAudioMidiDriver: request {} samples ({} total)", samples, requested);
});
```

**`get_controlled_mode_samples_to_process`**
```cpp
return m_rust->get_controlled_mode_samples_to_process();
```

**`start`**
Before spawning the thread, execute `this->m_command_queue.PROC_exec_all();` exactly as before.

The thread lambda becomes:
```cpp
m_proc_thread = std::thread([this] {
    Log::log<log_level_debug>("Starting process thread - {}", mode_names.at((DummyAudioMidiDriverMode)m_rust->get_mode()));
    auto bufs_per_second = AudioMidiDriver::get_sample_rate() / AudioMidiDriver::get_buffer_size();
    auto interval = 1.0f / ((float)bufs_per_second);
    auto micros = uint32_t(interval * 1000000.0f);
    float time_taken = 0.0f;
    while (!m_rust->is_finish()) {
        std::this_thread::sleep_for(std::chrono::microseconds((uint32_t)std::ceil(std::max(0.0f, micros - time_taken))));
        this->m_command_queue.PROC_handle_command_queue();
        if (!m_rust->is_paused()) {
            auto start = std::chrono::high_resolution_clock::now();
            auto mode = (DummyAudioMidiDriverMode)m_rust->get_mode();
            auto samples_to_process = m_rust->get_controlled_mode_samples_to_process();
            uint32_t to_process = mode == DummyAudioMidiDriverMode::Controlled ?
                std::min(samples_to_process, AudioMidiDriver::get_buffer_size()) :
                AudioMidiDriver::get_buffer_size();
            if (to_process > 0) {
                Log::log<log_level_debug_trace>("Process {}", to_process);
            }
            AudioMidiDriver::PROC_process(to_process);
            if (mode == DummyAudioMidiDriverMode::Controlled) {
                m_rust->controlled_mode_advance(to_process);
            }
            auto end = std::chrono::high_resolution_clock::now();
            time_taken = duration_cast<std::chrono::microseconds>(end - start).count();
        }
    }
    Log::log<log_level_debug>("Ending process thread");
});
AudioMidiDriver::set_active(true);
```

**`pause`**
```cpp
Log::log<log_level_debug>("DummyAudioMidiDriver: pause");
m_rust->pause();
wait_process();
```

**`resume`**
```cpp
Log::log<log_level_debug>("DummyAudioMidiDriver: resume");
m_rust->resume();
```

**`close`**
```cpp
m_rust->set_finish();
if (m_proc_thread.joinable()) {
    if (std::this_thread::get_id() != m_proc_thread.get_id()) {
        m_proc_thread.join();
    } else {
        m_proc_thread.detach();
    }
}
```

**`controlled_mode_run_request`**
Replace direct member reads with Rust queries:
```cpp
while(
    (DummyAudioMidiDriverMode)m_rust->get_mode() == DummyAudioMidiDriverMode::Controlled &&
    m_rust->get_controlled_mode_samples_to_process() > 0 &&
    !timed_out()
) { ... }

if (m_rust->get_controlled_mode_samples_to_process() > 0) { ... }
```

**`open_audio_port`, `open_midi_port`, `find_external_ports`, `add/remove external mock ports`**
No changes required.

---

## Phase 3: Build & Test

### Build
The project is built via Cargo. From the repo root:
```bash
cargo build
```
This triggers CMake, which compiles the C++ backend and links the Rust static library. If this is the final build step, first run:
```bash
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

### Rust tests
```bash
cargo test
```

### C++ backend tests
The C++ test runner is produced as a side effect of the Cargo build. Find it under `target/`:
```bash
find target -name test_runner -type f -executable
```
Run the DummyAudioMidiDriver-focused tests:
```bash
$(find target -name test_runner -type f -executable | head -1) "[DummyAudioMidiDriver]"
$(find target -name test_runner -type f -executable | head -1) "[DummyPorts]"
```
Run all backend tests:
```bash
$(find target -name test_runner -type f -executable | head -1)
```

### Application self-tests
Run the full QML self-test suite to verify no regressions at the application level:
```bash
./target/debug/shoopdaloop_dev.sh --self-test
```
On a headless machine or over SSH, force the offscreen platform:
```bash
QT_QPA_PLATFORM=offscreen ./target/debug/shoopdaloop_dev.sh --self-test
```
(Exact script name may vary by OS.)

---

## Files

### Create
| File | Purpose |
|------|---------|
| `src/rust/backend_rust/src/dummy_audio_midi_driver.rs` | Rust struct with atomics and business logic |
| `src/rust/backend_rust/src/dummy_audio_midi_driver_cxx.rs` | CXX bridge exposing Rust to C++ |

### Modify
| File | Change |
|------|--------|
| `src/rust/backend_rust/src/lib.rs` | Add `pub mod dummy_audio_midi_driver;` and `pub mod dummy_audio_midi_driver_cxx;` |
| `src/rust/backend_rust/build.rs` | Add bridge file to `cxx_build::bridges([...])` and `rerun-if-changed` |
| `src/backend/internal/DummyAudioMidiDriver.h` | Replace 4 member atomics with `rust::Box<backend_rust::DummyAudioMidiDriver>`; add CXX include |
| `src/backend/internal/DummyAudioMidiDriver.cpp` | Delegate all state access to `m_rust`; keep thread spawn in C++ |

### Do not modify
- `AudioMidiDriver.h` / `AudioMidiDriver.cpp`
- `JackAudioMidiDriver.h` / `JackAudioMidiDriver.cpp`
- `AudioMidiDrivers.cpp`
- `libshoopdaloop_backend.cpp`
- Any test source files

---

## Future-proofing for other AudioMidiDriver ports

This plan intentionally leaves `AudioMidiDriver` untouched. When a future port moves `JackAudioMidiDriver` to Rust, the same pattern can apply: keep the C++ template/leaf class as a thin wrapper inheriting from `AudioMidiDriver`, move state to Rust, and let the C++ side handle virtual dispatch and base-class interactions. The Rust core for `DummyAudioMidiDriver` is intentionally simple (plain atomics with no inheritance) so that a future `JackAudioMidiDriver` Rust core can reuse the same pattern without being tied to dummy-specific logic.
