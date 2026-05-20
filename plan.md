# Port Plan: InternalAudioPort and InternalMidiPort

## Overview

This plan details the port of two C++ internal port classes into Rust, while keeping thin C++ wrappers that maintain the existing inheritance hierarchy and integrate with the rest of the backend.

- **InternalAudioPort** — currently a C++ class that inherits `RustAudioPortF32`. It owns a local `std::vector<float>` buffer and delegates audio processing (gain, mute, peaks, ringbuffer) to the base class's Rust `AudioPort`. The port will be ported by moving its buffer management and metadata into a Rust core, with the C++ wrapper becoming a thin delegation layer.
- **InternalMidiPort** — does not yet exist in the generic backend. The public C API function `open_internal_midi_port` currently throws `std::runtime_error("Creating internal MIDI ports not yet supported")`. This port will be created from scratch, following the same Rust-core / C++-wrapper pattern used by `DummyMidiPort` and `MidiPort`.

Both ports follow the established project pattern:
- Rust owns the core data and real-time-safe logic.
- C++ provides a thin wrapper that inherits the necessary C++ base classes (`RustAudioPortF32`, `MidiPort`) so that graph nodes and the C API can continue to use shared pointers and virtual dispatch.
- CXX bridges expose the Rust types to C++.

## Code Locations

- **Rust backend crate**: `src/rust/backend_rust/src/`
- **C++ backend internals**: `src/backend/internal/`
- **C++ backend C API**: `src/backend/libshoopdaloop_backend.cpp`
- **C++ backend public header**: `src/backend/libshoopdaloop_backend.h`
- **CMake build**: `src/backend/CMakeLists.txt` (uses `file(GLOB INTERNAL_SOURCES .../*.cpp)`, so new `.cpp` files in `internal/` are picked up automatically)
- **Rust build script**: `src/rust/backend_rust/build.rs`
- **Existing C++ wrappers to study**:
  - `src/backend/internal/InternalAudioPort.h` and `.cpp`
  - `src/backend/internal/DummyAudioPort.h` and `.cpp`
  - `src/backend/internal/DummyMidiPort.h` and `.cpp`
  - `src/backend/internal/MidiPort.h` and `.cpp`
- **Existing Rust cores to study**:
  - `src/rust/backend_rust/src/dummy_audio_port.rs` and `_cxx.rs`
  - `src/rust/backend_rust/src/dummy_midi_port.rs` and `_cxx.rs`
  - `src/rust/backend_rust/src/audio_port.rs` and `_cxx.rs`
  - `src/rust/backend_rust/src/midi_port.rs` and `_cxx.rs`
  - `src/rust/backend_rust/src/midi_storage.rs` (for `MidiStorageElem`)
- **C++ tests**:
  - `src/backend/test/unit/test_InternalAudioPort.cpp`
  - `src/backend/test/integration/test_chain_single_midi_passthrough.cpp`

## Phase 1: InternalAudioPort

### 1.1 Rust Core

Create `src/rust/backend_rust/src/internal_audio_port.rs`.

Define `InternalAudioPort`:
- Fields:
  - `buffer: Vec<f32>` — local audio buffer
  - `name: String`
  - `input_connectability: u32`
  - `output_connectability: u32`
- Methods:
  - `new(name: &str, n_frames: u32, input_connectability: u32, output_connectability: u32) -> Self`
  - `name(&self) -> &str`
  - `proc_prepare(&mut self, nframes: u32)` — resizes `buffer` to at least `nframes`, then zero-fills the first `nframes` samples
  - `proc_get_buffer(&mut self, nframes: u32) -> *mut f32` — ensures `buffer` capacity, returns `buffer.as_mut_ptr()`. Safety: pointer is valid until the next mutating call on this instance.
  - `input_connectability(&self) -> u32`
  - `output_connectability(&self) -> u32`

Add Rust unit tests at the bottom of the file (inside `#[cfg(test)] mod tests`):
- Creation test
- Prepare zero-fills
- Buffer pointer is non-null after prepare
- Name round-trip

Create `src/rust/backend_rust/src/internal_audio_port_cxx.rs`.

Use a `#[cxx::bridge(namespace = "backend_rust")]` module named `ffi`:
- Expose `type InternalAudioPort;`
- Expose constructor: `fn new_internal_audio_port(name: &str, n_frames: u32, input_connectability: u32, output_connectability: u32) -> Box<InternalAudioPort>;`
- Expose `fn name(self: &InternalAudioPort) -> &str;`
- Expose `fn proc_prepare(self: &mut InternalAudioPort, nframes: u32);`
- Expose `unsafe fn proc_get_buffer(self: &mut InternalAudioPort, nframes: u32) -> *mut f32;`
- Expose `fn input_connectability(self: &InternalAudioPort) -> u32;`
- Expose `fn output_connectability(self: &InternalAudioPort) -> u32;`

Implement thin free-function wrappers that delegate to the struct methods (following the pattern in `dummy_audio_port_cxx.rs`).

Update `src/rust/backend_rust/src/lib.rs`:
- Add `pub mod internal_audio_port;`
- Add `pub mod internal_audio_port_cxx;`

Update `src/rust/backend_rust/build.rs`:
- Add `"src/internal_audio_port_cxx.rs"` to the `cxx_build::bridges([...])` array.
- Add `println!("cargo:rerun-if-changed=src/internal_audio_port_cxx.rs");`.

### 1.2 C++ Wrapper

Modify `src/backend/internal/InternalAudioPort.h`.

Keep the class declaration inheriting `RustAudioPortF32`, but replace direct data members with a single Rust box:

- Remove: `std::string m_name`, `std::vector<float> m_buffer`, `unsigned m_input_connectability`, `unsigned m_output_connectability`.
- Add: `rust::Box<backend_rust::InternalAudioPort> m_rust_internal;`

Keep the constructor signature exactly as before:
```cpp
InternalAudioPort(
    std::string name,
    uint32_t n_frames,
    unsigned input_connectability,
    unsigned output_connectability,
    shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> maybe_ringbuffer_buffer_pool
);
```

All virtual overrides must delegate to `m_rust_internal` or to the `RustAudioPortF32` base:
- `PROC_get_buffer(uint32_t n_frames) -> float*` — delegate to `m_rust_internal->proc_get_buffer(n_frames)`.
- `PROC_prepare(uint32_t nframes)` — delegate to `m_rust_internal->proc_prepare(nframes)`.
- `PROC_process(uint32_t nframes)` — get the buffer pointer via `PROC_get_buffer(nframes)`, then call `backend_rust::audio_port_process(**m_rust, buf, nframes)` (same as today, but using the Rust core's buffer). If `nframes == 0` or the base `m_rust` is not initialized, return early.
- `name() const -> const char*` — return `m_rust_internal->name().data()`.
- `type() const -> PortDataType` — return `PortDataType::Audio`.
- `close()` — no-op.
- `maybe_driver_handle() const -> void*` — return `(void*)this`.
- `get_external_connection_status() const` — return empty `PortExternalConnectionStatus`.
- `connect_external(std::string)` and `disconnect_external(std::string)` — throw `std::runtime_error` with message "Internal ports cannot be externally connected.".
- `input_connectability() const -> unsigned` — delegate to `m_rust_internal->input_connectability()`.
- `output_connectability() const -> unsigned` — delegate to `m_rust_internal->output_connectability()`.
- `has_internal_read_access() const` — keep returning `true`.
- `has_internal_write_access() const` — keep returning `true`.
- `has_implicit_input_source() const` — keep returning `false`.
- `has_implicit_output_sink() const` — keep returning `false`.

Modify `src/backend/internal/InternalAudioPort.cpp`.

Update the constructor to initialize `m_rust_internal` via `backend_rust::new_internal_audio_port(name, n_frames, input_connectability, output_connectability)` and call the `RustAudioPortF32` base constructor with the buffer pool.

Remove any now-unused direct member manipulation.

### 1.3 Verification

Run `cargo build`. Expect it to succeed.

Run `cargo test`. Expect the new Rust unit tests to pass.

Find the C++ `test_runner` executable (produced as a side-effect of the Cargo build, nested under `target/debug/` or `target/release/`). Run it and verify that the `InternalAudioPort` tests pass:
- `Ports - Internal Audio - Properties`
- `Ports - Internal Audio - Gain`
- `Ports - Internal Audio - Mute`
- `Ports - Internal Audio - Peak`
- `Ports - Internal Audio - Noop Zero`
- `Ports - Internal Audio - get ringbuffer data`

## Phase 2: InternalMidiPort

### 2.1 Rust Core

Create `src/rust/backend_rust/src/internal_midi_port.rs`.

Define `InternalMidiPort`:
- Fields:
  - `events: Vec<MidiStorageElem>` — local MIDI event buffer (cleared each prepare)
  - `name: String`
  - `input_connectability: u32`
  - `output_connectability: u32`
- Methods:
  - `new(name: &str, input_connectability: u32, output_connectability: u32) -> Self`
  - `name(&self) -> &str`
  - `proc_prepare(&mut self, _nframes: u32)` — clear `events`
  - `n_events(&self) -> u32`
  - `get_event_time(&self, idx: u32) -> u32` — return `INVALID_EVENT_TIME` (0xFFFFFFFF) if out of bounds
  - `get_event_size(&self, idx: u32) -> u16` — return `INVALID_EVENT_SIZE` (0xFFFF) if out of bounds
  - `get_event_bytes(&self, idx: u32, out: *mut u8, max_len: usize)` — unsafe; copies event bytes into `out` up to `max_len`
  - `write_event(&mut self, time: u32, size: u16, data: &[u8; 4])` — constructs a `MidiStorageElem` and pushes to `events`
  - `input_connectability(&self) -> u32`
  - `output_connectability(&self) -> u32`

The event-access pattern must match the existing `dummy_midi_port_cxx.rs` pattern: split into time/size/bytes because CXX cannot easily return a C-struct with an inline array by value across the bridge in all configurations.

Add Rust unit tests:
- Creation test
- Prepare clears events
- Write then read event round-trip
- Out-of-bounds access returns sentinel values

Create `src/rust/backend_rust/src/internal_midi_port_cxx.rs`.

Use a `#[cxx::bridge(namespace = "backend_rust")]` module named `ffi`:
- Expose `type InternalMidiPort;`
- Expose constructor: `fn new_internal_midi_port(name: &str, input_connectability: u32, output_connectability: u32) -> Box<InternalMidiPort>;`
- Expose `fn name(self: &InternalMidiPort) -> &str;`
- Expose `fn proc_prepare(self: &mut InternalMidiPort, nframes: u32);`
- Expose `fn n_events(self: &InternalMidiPort) -> u32;`
- Expose `fn get_event_time(self: &InternalMidiPort, idx: u32) -> u32;`
- Expose `fn get_event_size(self: &InternalMidiPort, idx: u32) -> u16;`
- Expose `unsafe fn get_event_bytes(self: &InternalMidiPort, idx: u32, out: *mut u8, max_len: usize);`
- Expose `fn write_event(self: &mut InternalMidiPort, time: u32, size: u16, data: &[u8; 4]);`
- Expose `fn input_connectability(self: &InternalMidiPort) -> u32;`
- Expose `fn output_connectability(self: &InternalMidiPort) -> u32;`

Implement thin free-function wrappers that delegate to the struct methods.

Update `src/rust/backend_rust/src/lib.rs`:
- Add `pub mod internal_midi_port;`
- Add `pub mod internal_midi_port_cxx;`

Update `src/rust/backend_rust/build.rs`:
- Add `"src/internal_midi_port_cxx.rs"` to the bridges array and add the rerun-if-changed line.

### 2.2 C++ Wrapper

Create `src/backend/internal/InternalMidiPort.h`.

Declare `InternalMidiPort` as:
```cpp
class InternalMidiPort : public MidiPort,
                         public MidiReadableBuffer,
                         public MidiWriteableBuffer,
                         private ModuleLoggingEnabled<"Backend.InternalMidiPort"> {
    rust::Box<backend_rust::InternalMidiPort> m_rust_internal;
public:
    InternalMidiPort(
        std::string name,
        unsigned input_connectability,
        unsigned output_connectability,
        unsigned ringbuffer_n_samples_hint = 0
    );

    // MidiReadableBuffer
    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;

    // MidiWriteableBuffer
    void write_event(MidiStorageElem event) override;

    // MidiPort buffer accessors — return this so MidiPort::PROC_prepare stores our buffer pointers
    MidiWriteableBuffer *PROC_get_write_data_into_port_buffer(uint32_t n_frames) override;
    MidiReadableBuffer *PROC_get_read_output_data_buffer(uint32_t n_frames) override;
    MidiReadableBuffer *PROC_internal_read_input_data_buffer(uint32_t n_frames) override;
    MidiWriteableBuffer *PROC_internal_write_output_data_to_buffer(uint32_t n_frames) override;

    // Processing
    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    // PortInterface
    const char* name() const override;
    PortDataType type() const override;
    void close() override;
    void* maybe_driver_handle() const override;
    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;
    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return false; }
    bool has_implicit_output_sink() const override { return false; }
    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
};
```

Create `src/backend/internal/InternalMidiPort.cpp`.

- Constructor: call `MidiPort(true, true, true)` base constructor, then initialize `m_rust_internal` with `backend_rust::new_internal_midi_port(name, input_connectability, output_connectability)`.
- `n_events()` — delegate to `m_rust_internal->n_events()`.
- `get_event(idx)` — read time, size, and bytes from the Rust bridge; reconstruct a `MidiStorageElem`. Follow exactly the pattern in `DummyMidiPort::get_event`.
- `write_event(event)` — copy event bytes into a `std::array<uint8_t, 4>`, then call `m_rust_internal->write_event(event.time, event.size, data_arr)`.
- `PROC_get_write_data_into_port_buffer` — return `static_cast<MidiWriteableBuffer*>(this)`.
- `PROC_get_read_output_data_buffer` — return `static_cast<MidiReadableBuffer*>(this)`.
- `PROC_internal_read_input_data_buffer` — return `static_cast<MidiReadableBuffer*>(this)`.
- `PROC_internal_write_output_data_to_buffer` — return `static_cast<MidiWriteableBuffer*>(this)`.
- `PROC_prepare(nframes)` — call `m_rust_internal->proc_prepare(nframes)`, then call `MidiPort::PROC_prepare(nframes)` so the base class updates its atomic buffer pointers to point at `this`.
- `PROC_process(nframes)` — call `MidiPort::PROC_process(nframes)`. Do not add extra logic; the base class handles ringbuffer, state tracking, and copying between our readable and writeable buffers.
- `name()` — return `m_rust_internal->name().data()`.
- `type()` — return `PortDataType::Midi`.
- `close()` — no-op.
- `maybe_driver_handle()` — return `(void*)this`.
- `get_external_connection_status()` — return empty `PortExternalConnectionStatus`.
- `connect_external` / `disconnect_external` — throw `std::runtime_error("Internal ports cannot be externally connected.")`.
- `input_connectability()` / `output_connectability()` — delegate to Rust core.

### 2.3 C API Integration

Modify `src/backend/libshoopdaloop_backend.cpp`.

Add `#include "InternalMidiPort.h"` near the other internal port includes.

Locate `open_internal_midi_port` (around line 1242). Replace the function body:

```cpp
shoopdaloop_midi_port_t *open_internal_midi_port (shoop_backend_session_t *backend, const char* name_hint, unsigned min_always_on_ringbuffer_samples) {
  return api_impl<shoopdaloop_midi_port_t*>("open_internal_midi_port", [&]() -> shoopdaloop_midi_port_t* {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return nullptr; }
    auto port = shoop_make_shared<InternalMidiPort>(
      std::string(name_hint),
      ShoopPortConnectability_Internal,
      ShoopPortConnectability_Internal,
      min_always_on_ringbuffer_samples
    );
    if (min_always_on_ringbuffer_samples > 0) {
      port->set_ringbuffer_n_samples(min_always_on_ringbuffer_samples);
    }
    auto pi = _backend->add_midi_port(port);
    _backend->set_graph_node_changes_pending();
    return external_midi_port(shoop_static_pointer_cast<GraphPort>(pi));
  }, (shoopdaloop_midi_port_t*) nullptr);
}
```

### 2.4 Verification

Run `cargo build`. Expect success.

Run `cargo test`. Expect the new Rust unit tests for `InternalMidiPort` to pass.

Build and run the C++ `test_runner`. Existing MIDI integration tests (e.g., `Chain - Midi port passthrough`) must still pass. `InternalMidiPort` must not break any graph wiring or buffer hand-off.

Optional but recommended: create `src/backend/test/unit/test_InternalMidiPort.cpp` with basic tests analogous to `test_InternalAudioPort.cpp`:
- Properties (read/write access flags)
- Write event, prepare clears, read back
- Mute passthrough via base `MidiPort`

Because CMake globs `.cpp` files in `src/backend/test/`, the new file will be picked up automatically on the next build.

## Phase 3: Cleanup and Final Verification

### 3.1 Remove Dead Code

In `InternalAudioPort.cpp`, ensure no leftover direct member manipulations remain.

In `InternalMidiPort.cpp`, ensure there are no stub or placeholder implementations.

### 3.2 Format

Run:
```bash
cargo fmt --all
```

### 3.3 Warnings-as-Errors Build

Run:
```bash
RUSTFLAGS="-D warnings" cargo build
```

### 3.4 Full Test Matrix

1. **Rust tests**: `cargo test`
2. **C++ tests**: Locate the `test_runner` executable under `target/debug/` or `target/release/` and run it. All backend unit and integration tests must pass.
3. **Application self-tests** (if available on the build host):
   ```bash
   QT_QPA_PLATFORM=offscreen ./target/debug/shoopdaloop_dev.sh --self-test
   ```

## Build & Test Reference (from `.agents/`)

- The complete project builds with `cargo build`.
- C++ tests compile automatically as a side-effect and produce a `test_runner` binary inside the Cargo target tree.
- Rust tests run with `cargo test`.
- For the final build step, always run `cargo fmt --all` first, then build with `RUSTFLAGS="-D warnings" cargo build`.
- If Qt tests fail with a platform error, prefix with `QT_QPA_PLATFORM=offscreen`.
