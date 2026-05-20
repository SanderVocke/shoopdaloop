# Port Plan: DecoupledMidiPort (C++ → Rust)

## Goal
Replace the C++ `DecoupledMidiPort` class with a Rust-backed implementation. The Rust side will own the lock-free SPSC message queue (replacing `boost::lockfree::spsc_queue` with `crossbeam_queue::ArrayQueue`). The C++ side becomes a thin wrapper that still orchestrates process-thread calls into the wrapped `MidiPort` and tracks the parent `AudioMidiDriver` via `shoop_weak_ptr`.

## Source Locations

- **C++ original**: `src/backend/internal/DecoupledMidiPort.h`, `src/backend/internal/DecoupledMidiPort.cpp`
- **C++ consumers**: `src/backend/internal/AudioMidiDriver.h`, `src/backend/internal/AudioMidiDriver.cpp`, `src/backend/internal/shoop_globals.h`, `src/backend/internal/libshoopdaloop_test_if.h`, `src/backend/libshoopdaloop_backend.cpp`
- **Rust crate**: `src/rust/backend_rust/`
- **Existing Rust patterns to follow**: `src/rust/backend_rust/src/dummy_midi_port.rs` + `dummy_midi_port_cxx.rs`, `src/rust/backend_rust/src/midi_storage_cxx.rs`

---

## Phase 1 — Rust Core (`src/rust/backend_rust/src/decoupled_midi_port.rs`)

Create a new module that owns the message queue and direction.

### Types and API

```rust
use crossbeam_queue::ArrayQueue;
use crate::midi_storage::MidiStorageElem;
use crate::port_direction::PortDirection;

pub struct DecoupledMidiPort {
    queue: ArrayQueue<MidiStorageElem>,
    direction: PortDirection,
}

impl DecoupledMidiPort {
    pub fn new(queue_size: usize, direction: PortDirection) -> Self;
    pub fn push(&self, msg: MidiStorageElem) -> bool;   // true if queued, false if full
    pub fn pop(&self) -> Option<MidiStorageElem>;
    pub fn is_empty(&self) -> bool;
    pub fn direction(&self) -> PortDirection;
}
```

### Rules
- `MidiStorageElem` is the existing `#[repr(C)]` struct from `midi_storage.rs` (already `Clone`).
- The queue must be bounded; silently drop messages when full (matching current C++ behavior where the `boost::lockfree::spsc_queue::push` return value is ignored).
- No logging in Rust; C++ wrapper retains `ModuleLoggingEnabled`.
- Add a small Rust unit test in the same file (creation, push/pop, empty, full drop).

---

## Phase 2 — CXX Bridge (`src/rust/backend_rust/src/decoupled_midi_port_cxx.rs`)

Create a CXX bridge that exposes the Rust type to C++. Follow the pattern used by `dummy_midi_port_cxx.rs` and `midi_storage_cxx.rs`.

### Bridge API

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type DecoupledMidiPort;

        fn new_decoupled_midi_port(queue_size: u32, direction: u32) -> Box<DecoupledMidiPort>;
        fn decoupled_push(self: &DecoupledMidiPort, time: u32, size: u16, data: &[u8; 4]) -> bool;
        unsafe fn decoupled_pop(self: &DecoupledMidiPort, out_time: &mut u32, out_size: &mut u16, out_data: *mut u8) -> bool;
        fn decoupled_is_empty(self: &DecoupledMidiPort) -> bool;
    }
}
```

### Notes
- `direction: u32` is mapped from `shoop_port_direction_t` values (`ShoopPortDirection_Input = 0`, `ShoopPortDirection_Output = 1`).
- `out_data: *mut u8` receives the 4 inline bytes. The C++ caller must ensure the pointer points to at least 4 writable bytes.
- The free-function wrappers inside the bridge file convert between `MidiStorageElem` and the decomposed primitive arguments.
- Add the new bridge file to `build.rs` in the `cxx_build::bridges([...])` array.

---

## Phase 3 — C++ Wrapper (`DecoupledMidiPort.h` / `.cpp`)

### `DecoupledMidiPort.h`
- Remove the `template<typename TimeType, typename SizeType>` parameterization. The class becomes a concrete, non-template class.
- Remove `#include <boost/lockfree/spsc_queue.hpp>`.
- Add `#include "backend_rust/src/decoupled_midi_port_cxx.rs.h"`.
- Replace the `boost::lockfree::spsc_queue` member with `rust::Box<backend_rust::DecoupledMidiPort> m_rust;`.
- Keep the `shoop_shared_ptr<MidiPort> port`, `shoop_port_direction_t direction`, `shoop_weak_ptr<AudioMidiDriver> maybe_driver`, and `ModuleLoggingEnabled` exactly as before.
- Keep the existing public method signatures unchanged so that call sites in `AudioMidiDriver`, `libshoopdaloop_backend.cpp`, etc. do not need modification.
- Remove the `extern template` declarations at the bottom.

### `DecoupledMidiPort.cpp`
- In the constructor, initialize `m_rust` with `backend_rust::new_decoupled_midi_port(queue_size, static_cast<uint32_t>(direction))`.
- In `PROC_process`:
  - **Input direction**: after calling `port->PROC_prepare` / `port->PROC_process`, read events from `port->PROC_get_read_output_data_buffer(n_frames)`. For each event, call `m_rust->decoupled_push(event.time, event.size, event.bytes)`.
  - **Output direction**: after `port->PROC_prepare`, call `m_rust->decoupled_pop` in a loop to drain queued events into `port->PROC_get_write_data_into_port_buffer(n_frames)`, then call `port->PROC_process`.
- In `pop_incoming`: call `m_rust->decoupled_pop` and return `std::optional<Message>`.
- In `push_outgoing`: call `m_rust->decoupled_push`.
- Remove all `template <typename TimeType, typename SizeType>` prefixes and the explicit template instantiations at the bottom.
- Keep `name()`, `close()`, `get_maybe_driver()`, `forget_driver()`, and `get_port()` exactly as they are today.

---

## Phase 4 — Update Type Aliases and Forward Declarations

### `src/backend/internal/shoop_globals.h`
- Change `template<typename A, typename B> class DecoupledMidiPort;` → `class DecoupledMidiPort;`
- Change `using _DecoupledMidiPort = DecoupledMidiPort<Time, Size>;` → `using _DecoupledMidiPort = DecoupledMidiPort;`

### Other files
- `AudioMidiDriver.h`, `AudioMidiDriver.cpp`, `libshoopdaloop_backend.cpp`, and `libshoopdaloop_test_if.h` already refer to `_DecoupledMidiPort`, so they should need no changes.
- Verify no other file uses the template syntax `DecoupledMidiPort<...>`.

---

## Phase 5 — Crate Registration

### `src/rust/backend_rust/src/lib.rs`
- Add `pub mod decoupled_midi_port;` and `pub mod decoupled_midi_port_cxx;`

### `src/rust/backend_rust/build.rs`
- Add `"src/decoupled_midi_port_cxx.rs"` to the `cxx_build::bridges([...])` array.
- Add a `println!("cargo:rerun-if-changed=src/decoupled_midi_port_cxx.rs");` line.

---

## Phase 6 — Build and Test

### Build commands
1. `cargo build` — compiles the Rust crate including the new CXX bridge.
2. If it is the final build step: `cargo fmt --all` then `RUSTFLAGS="-D warnings" cargo build`.

### Test commands
1. `cargo test` — runs Rust unit tests (should include the new `decoupled_midi_port` tests).
2. The C++ `test_runner` executable is produced as a side effect of building the `backend` crate. Find it under `target/debug/**/test_runner` (exact path varies by platform) and run it.
3. Run the QML self-tests with `./target/debug/shoopdaloop_dev.sh --self-test` (or the platform-specific dev script).

### Test coverage to verify
- Rust unit tests for queue push/pop/empty/full behavior.
- C++ `test_runner` still passes (no regressions in backend tests).
- QML self-tests still pass (end-to-end integration).

### Completion criteria
- `DecoupledMidiPort.cpp` no longer includes `<boost/lockfree/spsc_queue.hpp>`.
- `cargo fmt --all` produces no changes.
- `RUSTFLAGS="-D warnings" cargo build` succeeds with zero warnings.
- All tests pass.

---

## Key Design Decisions

1. **Why keep C++ wrapper?** `DecoupledMidiPort` needs to call C++ `MidiPort` methods (`PROC_prepare`, `PROC_process`, buffer accessors) and hold a `shoop_weak_ptr<AudioMidiDriver>`. Moving these references into Rust would require callbacks or a heavier FFI redesign. The pragmatic pattern used elsewhere in this repo (e.g. `DummyMidiPort`, `MidiPort`) is: Rust owns the data, C++ owns the wiring.
2. **Why concrete class instead of template?** The template parameters `TimeType` and `SizeType` were never used by the class body. Removing them simplifies the port and eliminates four explicit instantiations.
3. **Queue drop policy**: When `ArrayQueue` is full, `push` returns `Err(msg)`. The C++ wrapper discards the message, matching the current behavior where `boost::lockfree::spsc_queue::push` return value is ignored.
