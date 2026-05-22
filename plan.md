# Plan: Rust Port of Dummy Ports

## Overview

This plan describes the porting of the C++ dummy port infrastructure to Rust, following the established pattern in the codebase: port core logic to Rust, wrap it in a CXX bridge, and expose a thin C++ wrapper that satisfies the existing interfaces (`PortInterface`, `MidiPort`, etc.).

The dummy ports are `DummyAudioPort` and `DummyMidiPort`. Both are owned by `DummyAudioMidiDriver` and are used extensively in unit and integration tests. They are also the only port types that expose test-specific APIs (queueing/dequeuing data, requesting frames, clearing queues) that the public C API accesses via `dynamic_cast`.

**Already ported to Rust:**
- `AudioPort` (core logic) — wrapped by C++ `RustAudioPortF32`
- `MidiPort` (core logic) — wrapped by C++ `MidiPort`
- `DummyMidiPort` (core logic) — wrapped by C++ `DummyMidiPort`
- `MidiStorage`, `MidiStateTracker`, `MidiSortingBuffer`, etc.

**Remaining on the C++ side (target of this plan):**
- `DummyExternalConnections` — mock external port registry and connection tracking
- `DummyPortCore` — shared port metadata (name, direction, external connections)
- `DummyAudioPort` — audio port with sample queue/dequeue logic (wraps Rust `AudioPort` but keeps queueing in C++)
- `DummyMidiPort` C++ wrapper — thin but still holds `MidiPort` base class and buffer interfaces

**Architecture constraint:** other port types (JACK, etc.) will be ported later. The `DummyPortCore` abstraction is intentionally shared — JACK ports will also need name/direction/external-connection metadata. Design the Rust side so that `DummyPortCore` (or a renamed generalization) can be composed into any future port type.

---

## Step 1: Port `DummyExternalConnections` to Rust

`DummyExternalConnections` is a standalone struct with no inheritance. It maintains a list of mock external ports (`ExternalPortDescriptor`) and a list of active connections (`(DummyPortCore*, name)` pairs). It is owned by `DummyAudioMidiDriver` and passed weakly to every `DummyPortCore`.

### 1.1 Create `src/rust/backend_rust/src/dummy_external_connections.rs`

Implement a Rust struct with equivalent behavior:

```rust
pub struct DummyExternalConnections {
    mock_ports: Vec<ExternalPortDescriptor>,
    // Connections: (port_ptr_as_usize, external_port_name)
    // Storing as usize because CXX cannot hold raw pointers to opaque types.
    connections: Vec<(usize, String)>,
}
```

Methods to port:
- `add_external_mock_port(name, direction, data_type)`
- `remove_external_mock_port(name)`
- `remove_all_external_mock_ports()`
- `connect(port_ptr, external_port_name)` — takes `usize` port pointer
- `disconnect(port_ptr, external_port_name)` — takes `usize` port pointer
- `get_port(name)` — returns reference (or panic, matching C++)
- `find_external_ports(maybe_regex, direction_filter, data_type_filter)`
- `connection_status_of(port_ptr)` — returns `PortExternalConnectionStatus` map

The `PortExternalConnectionStatus` type is `std::map<std::string, bool>` in C++. In Rust, return `Vec<(String, bool)>` from the bridge and reconstruct the map on the C++ side, or define a shared struct. Follow the existing pattern in `midi_storage_cxx.rs` for returning collections.

### 1.2 Create `src/rust/backend_rust/src/dummy_external_connections_cxx.rs`

Write the CXX bridge exposing the Rust struct to C++:

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type DummyExternalConnections;

        fn new_dummy_external_connections() -> Box<DummyExternalConnections>;

        fn add_external_mock_port(
            self: &mut DummyExternalConnections,
            name: &str,
            direction: u32, // shoop_port_direction_t as raw value
            data_type: u32, // shoop_port_data_type_t as raw value
        );

        fn remove_external_mock_port(self: &mut DummyExternalConnections, name: &str);
        fn remove_all_external_mock_ports(self: &mut DummyExternalConnections);

        fn connect(self: &mut DummyExternalConnections, port_ptr: usize, external_port_name: &str);
        fn disconnect(self: &mut DummyExternalConnections, port_ptr: usize, external_port_name: &str);

        // Return connection status as a list of (name, connected) pairs
        fn connection_status_of(
            self: &DummyExternalConnections,
            port_ptr: usize,
        ) -> Vec<ConnectionStatusPair>;

        struct ConnectionStatusPair {
            name: String,
            connected: bool,
        }

        fn find_external_ports(
            self: &DummyExternalConnections,
            maybe_name_regex: &str,
            direction_filter: u32,
            data_type_filter: u32,
        ) -> Vec<ExternalPortDescriptor>;

        struct ExternalPortDescriptor {
            name: String,
            direction: u32,
            data_type: u32,
        }
    }
}
```

### 1.3 Update C++ `DummyExternalConnections` to delegate to Rust

Replace the body of `DummyExternalConnections` with a thin wrapper around `rust::Box<backend_rust::DummyExternalConnections>`:

```cpp
struct DummyExternalConnections : private ModuleLoggingEnabled<"Backend.DummyExternalConnections"> {
    rust::Box<backend_rust::DummyExternalConnections> m_rust;

    DummyExternalConnections() : m_rust(backend_rust::new_dummy_external_connections()) {}

    void add_external_mock_port(...) { m_rust->add_external_mock_port(...); }
    // ... etc
};
```

**Important:** the `DummyPortCore*` pointers stored in connections become `usize` opaque identifiers on the Rust side. The Rust code does not dereference them — it only uses them for equality comparison in `connection_status_of` and for insertion/removal in `connect`/`disconnect`. This is safe because the C++ side guarantees the pointer remains valid for the lifetime of the connection.

### 1.4 Register in `lib.rs`

Add `pub mod dummy_external_connections;` and `pub mod dummy_external_connections_cxx;` to `src/rust/backend_rust/src/lib.rs`.

### 1.5 Verify tests compile and pass

`DummyExternalConnections` is tested indirectly through `test_DummyAudioMidiDriver.cpp` and integration tests. Ensure all tests still pass.

---

## Step 2: Port `DummyPortCore` to Rust

`DummyPortCore` is a struct holding name, direction, external connections (weak), and a driver handle pointer. It is composed into both `DummyAudioPort` and `DummyMidiPort`. Future port types (JACK, etc.) will also need this metadata.

### 2.1 Create `src/rust/backend_rust/src/port_core.rs` (general name)

Rename concept from "DummyPortCore" to a general `PortCore` — this struct is useful for any port type, not just dummy ports.

```rust
pub struct PortCore {
    name: String,
    direction: PortDirection,
    // External connections are managed on the C++ side; we store an opaque handle
    external_connections_ptr: usize,
    driver_handle: usize,
}
```

The `PortDirection` enum already exists in `dummy_midi_port.rs`. Move it to a shared module (e.g., `src/rust/backend_rust/src/port_direction.rs`) so both `port_core.rs` and `dummy_midi_port.rs` can use it.

Methods:
- `name() -> &str`
- `direction() -> PortDirection`
- `close()` — no-op in Rust; C++ wrapper calls the destructor logic if needed
- `maybe_driver_handle() -> usize` — returns the stored opaque handle
- `get_external_connection_status() -> Vec<(String, bool)>` — delegates to the external connections object via C++ callback or stores a closure

The `get_external_connection_status` method is tricky because it needs to call into `DummyExternalConnections` which may be on the C++ side. Options:

**Option A (recommended):** The Rust `PortCore` stores a callback function pointer (or a trait object) for external connection queries. The C++ wrapper sets this callback during construction to a lambda that calls the C++ `DummyExternalConnections`.

**Option B:** Keep `get_external_connection_status` on the C++ wrapper only, and do not port it to Rust. The Rust `PortCore` only stores the metadata; connection status queries happen on the C++ side.

Given that `DummyExternalConnections` is also being ported to Rust (Step 1), Option A is feasible: store a raw pointer to the Rust `DummyExternalConnections` and call its methods directly. However, this creates a circular dependency if `DummyExternalConnections` stores `PortCore` pointers. Resolve this by using opaque `usize` handles on both sides.

**Decision:** Use Option B for simplicity. `PortCore` in Rust stores name, direction, driver_handle, and an opaque `usize` for external connections. The C++ wrapper implements `get_external_connection_status` by casting the opaque handle back to the C++ `DummyExternalConnections` (or the Rust one via another bridge call). This avoids circular dependencies and keeps the FFI surface minimal.

### 2.2 Create `src/rust/backend_rust/src/port_core_cxx.rs`

CXX bridge:

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type PortCore;
        type PortDirection;

        fn new_port_core(name: &str, is_output: bool, driver_handle: usize) -> Box<PortCore>;

        fn name(self: &PortCore) -> &str;
        fn is_output(self: &PortCore) -> bool;
        fn maybe_driver_handle(self: &PortCore) -> usize;

        fn set_external_connections_ptr(self: &mut PortCore, ptr: usize);
    }
}
```

### 2.3 Update C++ `DummyPortCore` to delegate to Rust

Replace the C++ `DummyPortCore` struct with a thin wrapper:

```cpp
struct DummyPortCore {
    rust::Box<backend_rust::PortCore> m_rust;
    shoop_weak_ptr<DummyExternalConnections> m_external_connections_cpp; // kept for get_external_connection_status

    DummyPortCore(std::string name, shoop_port_direction_t direction, void* driver_handle, shoop_weak_ptr<DummyExternalConnections> external_connections)
        : m_rust(backend_rust::new_port_core(name, direction == ShoopPortDirection_Output, reinterpret_cast<size_t>(driver_handle))),
          m_external_connections_cpp(external_connections) {
        m_rust->set_external_connections_ptr(reinterpret_cast<size_t>(this));
    }

    const char* name() const { return m_rust->name().data(); }
    void* maybe_driver_handle() const { return reinterpret_cast<void*>(m_rust->maybe_driver_handle()); }
    // ... etc
};
```

Note: `get_external_connection_status` remains implemented on the C++ side using `m_external_connections_cpp` because it needs to interact with the (possibly still C++-side) `DummyExternalConnections`.

### 2.4 Register in `lib.rs`

Add `pub mod port_direction;`, `pub mod port_core;`, `pub mod port_core_cxx;` to `lib.rs`.

### 2.5 Verify tests compile and pass

`DummyPortCore` is tested indirectly. Ensure no regressions.

---

## Step 3: Port `DummyAudioPort` Core Logic to Rust

`DummyAudioPort` is the most complex remaining port. It currently:
1. Inherits from `RustAudioPortF32` (which wraps Rust `AudioPort` for peak/gain/mute/ringbuffer)
2. Composes `DummyPortCore` (name/direction/external connections)
3. Composes `CommandQueue`
4. Owns a `boost::lockfree::spsc_queue<std::vector<audio_sample_t>>` for input sample queueing
5. Owns `m_retained_samples` and `m_buffer_data` vectors for output sample buffering
6. Implements `PROC_prepare`, `PROC_process`, `PROC_get_buffer`, `queue_data`, `dequeue_data`, `request_data`, `get_queue_empty`

The goal is to move items 4–6 to Rust, leaving the C++ side as a thin wrapper that:
- Inherits from `RustAudioPortF32` (to satisfy the `PortInterface` vtable for the graph system)
- Holds a `rust::Box<backend_rust::DummyAudioPort>` for the Rust core
- Delegates dummy-specific methods (`queue_data`, `dequeue_data`, etc.) to Rust
- Keeps `PROC_prepare` / `PROC_process` / `PROC_get_buffer` as C++ overrides that call Rust and then delegate to `RustAudioPortF32::PROC_process`

### 3.1 Create `src/rust/backend_rust/src/dummy_audio_port.rs`

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use crate::port_core::PortCore;
use crate::audio_port::AudioPort;

pub struct DummyAudioPort {
    // Port metadata (replaces DummyPortCore)
    pub port_core: PortCore,

    // Input sample queue: Vec of sample frames to be fed into the port
    queued_data: Vec<Vec<f32>>,

    // Retained samples for dequeue_data
    retained_samples: Vec<f32>,

    // Current buffer (PROC_get_buffer returns a pointer into here)
    buffer_data: Vec<f32>,

    // Requested sample count for dequeue
    n_requested_samples: AtomicU32,
}
```

Methods to implement:
- `new(name, direction, driver_handle) -> Self`
- `queue_data(n_frames, data: &[f32])` — append to `queued_data`
- `get_queue_empty() -> bool` — check if `queued_data` is empty
- `request_data(n_frames)` — increment `n_requested_samples`
- `dequeue_data(n) -> Vec<f32>` — take from `retained_samples`
- `PROC_prepare(nframes)` — drain `queued_data` into `buffer_data`, zero-fill remainder
- `PROC_process(nframes)` — buffer data is already filled by prepare; just decrement `n_requested_samples` and move to `retained_samples`
- `PROC_get_buffer(nframes) -> *mut f32` — ensure `buffer_data` is large enough, return pointer

The `AudioPort` (peak/gain/mute/ringbuffer) processing is done by the C++ `RustAudioPortF32` base class. The Rust `DummyAudioPort` does NOT need to own an `AudioPort` — it only manages the sample queue/buffer logic. The C++ wrapper will call `backend_rust::audio_port_process()` on the `RustAudioPortF32`'s inner Rust `AudioPort` after the buffer is prepared.

However, for a cleaner architecture, the Rust `DummyAudioPort` CAN own an `AudioPort` and expose a `process_raw(buffer_ptr, n_frames)` method that the C++ wrapper calls. This mirrors how `DummyMidiPort` owns a `MidiPort`. This makes the Rust side more self-contained.

**Decision:** The Rust `DummyAudioPort` should own an `AudioPort` (for gain/mute/peaks/ringbuffer) AND the sample queue logic. The C++ wrapper creates the Rust `DummyAudioPort` with a buffer pool configuration, and delegates `PROC_process` to Rust which handles both the sample queueing and the audio processing.

### 3.2 Create `src/rust/backend_rust/src/dummy_audio_port_cxx.rs`

CXX bridge:

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type DummyAudioPort;

        fn new_dummy_audio_port(
            name: &str,
            is_output: bool,
            driver_handle: usize,
            pool_capacity: usize,
            low_water_mark: usize,
            buffer_size: usize,
            max_buffers: u32,
        ) -> Box<DummyAudioPort>;

        // PortInterface delegation
        fn name(self: &DummyAudioPort) -> &str;
        fn maybe_driver_handle(self: &DummyAudioPort) -> usize;

        // Dummy-specific methods
        fn queue_data(self: &mut DummyAudioPort, n_frames: u32, data: &[f32]);
        fn get_queue_empty(self: &DummyAudioPort) -> bool;
        fn request_data(self: &mut DummyAudioPort, n_frames: u32);
        fn dequeue_data(self: &mut DummyAudioPort, n: u32) -> Vec<f32>;

        // Processing
        fn PROC_prepare(self: &mut DummyAudioPort, nframes: u32);
        unsafe fn PROC_process(self: &mut DummyAudioPort, buffer: *mut f32, nframes: u32);
        unsafe fn PROC_get_buffer(self: &mut DummyAudioPort, nframes: u32) -> *mut f32;

        // AudioPort delegation
        fn set_gain(self: &DummyAudioPort, gain: f32);
        fn get_gain(self: &DummyAudioPort) -> f32;
        fn set_muted(self: &mut DummyAudioPort, muted: bool);
        fn get_muted(self: &DummyAudioPort) -> bool;
        fn get_input_peak(self: &DummyAudioPort) -> f32;
        fn reset_input_peak(self: &DummyAudioPort);
        fn get_output_peak(self: &DummyAudioPort) -> f32;
        fn reset_output_peak(self: &DummyAudioPort);
        fn set_ringbuffer_n_samples(self: &mut DummyAudioPort, n: u32);
        fn get_ringbuffer_n_samples(self: &DummyAudioPort) -> u32;
    }
}
```

Note: the `PROC_process` in Rust takes a raw buffer pointer because the actual audio data lives in the C++-managed buffer (from `PROC_get_buffer`). The Rust side applies gain/mute/peaks via the owned `AudioPort`.

### 3.3 Rewrite C++ `DummyAudioPort` as a thin wrapper

The C++ `DummyAudioPort` becomes:

```cpp
class DummyAudioPort : public RustAudioPortF32,
                       private ModuleLoggingEnabled<"Backend.DummyAudioPort"> {
    rust::Box<backend_rust::DummyAudioPort> m_rust;
    CommandQueue m_command_queue;

public:
    DummyAudioPort(...)
        : RustAudioPortF32(buffer_pool, 32),
          m_rust(backend_rust::new_dummy_audio_port(...)),
          m_command_queue(100, 1000, 1000) {}

    // PortInterface — delegate to Rust for name/handle, to RustAudioPortF32 for rest
    const char* name() const override { return m_rust->name().data(); }
    void* maybe_driver_handle() const override { return reinterpret_cast<void*>(m_rust->maybe_driver_handle()); }

    // Dummy-specific — delegate to Rust via command queue
    void queue_data(uint32_t n_frames, audio_sample_t const* data) {
        m_command_queue.queue_and_wait([this, n_frames, data]() {
            // Convert data to slice and call Rust
            std::vector<float> v(data, data + n_frames);
            m_rust->queue_data(n_frames, v);
        });
    }

    // ... etc for request_data, dequeue_data, get_queue_empty

    // Processing
    void PROC_prepare(uint32_t nframes) override {
        m_command_queue.PROC_exec_all();
        m_rust->PROC_prepare(nframes);
        // Note: buffer filling happens in Rust now
    }

    void PROC_process(uint32_t nframes) override {
        auto buf = PROC_get_buffer(nframes);
        m_rust->PROC_process(buf, nframes);
        // Retained samples are handled in Rust
    }

    float* PROC_get_buffer(uint32_t n_frames) override {
        return m_rust->PROC_get_buffer(n_frames);
    }
};
```

**Important:** The C++ `DummyAudioPort` still inherits from `RustAudioPortF32` because the graph system and other C++ code use it polymorphically via `PortInterface*`. The `RustAudioPortF32` base class constructor creates its own `rust::Box<backend_rust::AudioPort>`. After this port, the `DummyAudioPort` will own TWO Rust audio ports: one in `RustAudioPortF32::m_rust` and one in `m_rust` (the `DummyAudioPort` Rust struct). This is wasteful.

**Better approach:** Make `DummyAudioPort` NOT inherit from `RustAudioPortF32`. Instead, make it inherit directly from `PortInterface` and delegate all `PortInterface` methods to the Rust `DummyAudioPort` which internally owns an `AudioPort`. This eliminates the double `AudioPort` ownership.

However, this requires that `RustAudioPortF32` can be used as a member rather than a base class. Currently `RustAudioPortF32` is a base class with virtual methods. To use it as a member, we'd need to either:
1. Make `DummyAudioPort` hold a `rust::Box<backend_rust::AudioPort>` directly and re-implement the `PortInterface` methods that `RustAudioPortF32` provides
2. Or keep `RustAudioPortF32` as base for now and accept the double ownership as a temporary measure

**Decision for this plan:** Keep `RustAudioPortF32` as the base class for now to minimize disruption. The double `AudioPort` ownership is a temporary wart that will be resolved when `RustAudioPortF32` itself is refactored to composition in a later pass. The Rust `DummyAudioPort` will NOT own an `AudioPort`; instead, the C++ wrapper will call `backend_rust::audio_port_process()` on the base class's `AudioPort` after calling Rust `DummyAudioPort::PROC_prepare()`.

This means the Rust `DummyAudioPort` only handles the sample queueing/buffering logic. The audio processing (gain/mute/peaks/ringbuffer) stays in the existing `RustAudioPortF32` / `AudioPort` pipeline.

### 3.4 Update tests

The existing tests in `test_DummyPorts.cpp` use `DummyAudioPort` directly. They should continue to work with the refactored wrapper. The `PROC_prepare` / `PROC_process` / `queue_data` / `dequeue_data` semantics must be preserved exactly.

Add new Rust unit tests in `dummy_audio_port.rs` to test the Rust core independently.

### 3.5 Register in `lib.rs`

Add `pub mod dummy_audio_port;` and `pub mod dummy_audio_port_cxx;`.

### 3.6 Verify all tests compile and pass

Run `cargo test -p backend_rust` and the C++ test suite.

---

## Step 4: Thin Down or Eliminate C++ `DummyMidiPort` Wrapper

`DummyMidiPort` already has its core logic in Rust (`dummy_midi_port.rs`). The C++ wrapper:
- Inherits from `MidiPort` (for the graph / ringbuffer / state tracking)
- Inherits from `MidiReadableBuffer` and `MidiWriteableBuffer`
- Composes `DummyPortCore`
- Composes `CommandQueue`
- Delegates dummy-specific methods to `m_rust` (the Rust `DummyMidiPort`)

After Step 2 (`DummyPortCore` ported to Rust), the C++ wrapper should be updated to use the Rust `PortCore` instead of the C++ `DummyPortCore`.

### 4.1 Update C++ `DummyMidiPort` to use Rust `PortCore`

Replace `DummyPortCore m_dummy_port_core` with a reference to the Rust `DummyMidiPort`'s internal `PortCore`. If the Rust `DummyMidiPort` stores a `PortCore`, the C++ wrapper can access it via the CXX bridge.

Alternatively, keep the C++ `DummyPortCore` wrapper (from Step 2) and have it delegate to the Rust `PortCore`. This is less invasive.

### 4.2 Evaluate whether the `MidiPort` base class can be composed

Currently `DummyMidiPort` inherits from `MidiPort` because:
1. The graph system needs `PortInterface*`
2. `MidiPort` provides ringbuffer, state tracking, event counting
3. Other code holds `shoop_shared_ptr<MidiPort>` to dummy MIDI ports

If `MidiPort` were refactored to allow composition (i.e., a port could hold a `MidiPort` member instead of inheriting from it), then `DummyMidiPort` could become a pure composition class. This is a larger refactor that affects `MidiBufferingInputPort`, `MidiSortingReadWritePort`, `JackMidiPort`, etc.

**Decision for this plan:** Do NOT refactor `MidiPort` inheritance in this pass. Keep `DummyMidiPort : MidiPort` and update it to use the Rust `DummyMidiPort` + Rust `PortCore` (via the C++ `DummyPortCore` wrapper). The `MidiPort` base class refactor is a separate future effort.

### 4.3 Verify tests compile and pass

`test_DummyPorts.cpp` and all MIDI-related integration tests should pass without changes.

---

## Step 5: Update the Public C API (`libshoopdaloop_backend.cpp`)

The public C API uses `dynamic_cast<DummyAudioPort*>` and `dynamic_cast<DummyMidiPort*>` to access dummy-specific methods. After the ports are fully wrapped, these casts still work because the C++ wrapper classes still exist with the same inheritance hierarchy.

However, if in the future `DummyAudioPort` stops inheriting from `RustAudioPortF32` (Step 3.3, better approach), the `dynamic_cast` path may need adjustment. For now, no changes are needed to `libshoopdaloop_backend.cpp`.

If `DummyAudioPort` is refactored to not inherit from `RustAudioPortF32`, then `internal_audio_port(port)` would need to return a different type, or the C API would need to use a different mechanism to reach the dummy port. This is out of scope for this plan.

---

## Step 6: Port `DummyAudioMidiDriver` to Rust (optional, future)

`DummyAudioMidiDriver` is the owner of the dummy ports. It:
- Owns `DummyExternalConnections`
- Owns sets of `DummyAudioPort` and `DummyMidiPort` shared pointers
- Spawns a process thread with controlled/auto mode
- Manages sample rate, buffer size, client name
- Implements `open_audio_port`, `open_midi_port`, `find_external_ports`

Porting `DummyAudioMidiDriver` to Rust is a significant effort because it involves:
- Thread management (`std::thread` → Rust `std::thread` or async)
- `AudioMidiDriver` base class refactor (processor list, decoupled MIDI ports, command queue)
- Template parameter handling (`DummyAudioMidiDriver<uint32_t, uint16_t>`, etc.)
- Interaction with the JACK driver base class (`AudioMidiDriver`)

**Decision:** Leave `DummyAudioMidiDriver` on the C++ side for now. It is not a blocker for porting the ports themselves. The driver can be ported later as a separate effort.

---

## Step 7: Final Verification and Cleanup

### 7.1 Run the full test suite

```bash
cargo test -p backend_rust
cmake --build build  # or cargo build -p backend
ctest  # or equivalent test runner
```

### 7.2 Check for dead code

Remove any C++ `.cpp`/`.h` files that are fully superseded by Rust implementations (e.g., if `DummyPort.cpp` becomes empty after Step 2).

### 7.3 Update documentation

Update `src/rust/backend_rust/README.md` or inline comments to document the new modules.

### 7.4 Code review checklist

- No raw pointers cross the FFI boundary unsafely
- All `unsafe` blocks in Rust are justified and documented
- C++ wrappers are thin (only delegation, no business logic)
- The `PortCore` abstraction is usable by future port types
- Tests cover both Rust unit tests and C++ integration tests

---

## Appendix A: File Changes Summary

### New Rust files
- `src/rust/backend_rust/src/port_direction.rs`
- `src/rust/backend_rust/src/port_core.rs`
- `src/rust/backend_rust/src/port_core_cxx.rs`
- `src/rust/backend_rust/src/dummy_external_connections.rs`
- `src/rust/backend_rust/src/dummy_external_connections_cxx.rs`
- `src/rust/backend_rust/src/dummy_audio_port.rs`
- `src/rust/backend_rust/src/dummy_audio_port_cxx.rs`

### Modified C++ files
- `src/backend/internal/DummyPort.h` / `.cpp` — `DummyPortCore` becomes thin wrapper
- `src/backend/internal/DummyPort.h` / `.cpp` — `DummyExternalConnections` becomes thin wrapper
- `src/backend/internal/DummyAudioPort.h` / `.cpp` — delegates to Rust `DummyAudioPort`
- `src/backend/internal/DummyMidiPort.h` / `.cpp` — uses Rust `PortCore` (optional in this pass)
- `src/rust/backend_rust/src/lib.rs` — register new modules

### Modified Rust files
- `src/rust/backend_rust/src/dummy_midi_port.rs` — use shared `PortDirection` from `port_direction.rs`

---

## Appendix B: Future Work (out of scope for this plan)

1. **Refactor `RustAudioPortF32` to composition** — eliminate the need for `DummyAudioPort` to inherit from it
2. **Refactor `MidiPort` to composition** — allow `DummyMidiPort` to compose rather than inherit
3. **Port `AudioMidiDriver` / `DummyAudioMidiDriver` to Rust** — full driver port
4. **Port JACK ports to Rust** — reuse `PortCore` and `AudioPort` / `MidiPort` Rust cores
5. **Unify `PortDirection` across all Rust modules** — currently duplicated in `dummy_midi_port.rs`
