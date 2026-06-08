# Plan: Fully port `DecoupledMidiPort` to Rust

## Objective

Port the backend `DecoupledMidiPort` object fully to Rust. After this task, there must be no C++ `DecoupledMidiPort` implementation/header in `src/backend/internal`; C++ code, the raw C API, tests, and driver code must interact with decoupled MIDI ports only through Rust-side bridge-object handles.

The existing Rust queue implementation in `src/rust/backend_rust/src/decoupled_midi_port.rs` should become part of the full Rust object. The final Rust object should own the underlying C++ `MidiPort` through a C++ bridge-object handle, expose Rust strong/weak bridge wrappers for lifetime management, and be registered/processed directly by the Rust `AudioMidiDriverCore`.

## Current relevant code

The current C++ object is in:

- `src/backend/internal/DecoupledMidiPort.h`
- `src/backend/internal/DecoupledMidiPort.cpp`

The current Rust queue-only implementation is in:

- `src/rust/backend_rust/src/decoupled_midi_port.rs`
- `src/rust/backend_rust/src/decoupled_midi_port_cxx.rs`

The current bridge declarations for the C++ `DecoupledMidiPort` object are in:

- `src/rust/backend_rust/src/cpp_decoupled_midi_port_cxx.rs`
- `src/rust/backend_rust/src/decoupled_midi_port_bridge_cxx.rs`

The current driver registration path is in:

- `src/backend/internal/AudioMidiDriver.h`
- `src/backend/internal/AudioMidiDriver.cpp`
- `src/rust/backend_rust/src/audio_midi_driver.rs`
- `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`

The current raw C API handle conversion and API functions are in:

- `src/backend/libshoopdaloop_backend.cpp`
- `src/backend/internal/libshoopdaloop_test_if.h`

The derived driver open paths are in:

- `src/backend/internal/DummyAudioMidiDriver.h`
- `src/backend/internal/DummyAudioMidiDriver.cpp`
- `src/backend/internal/jack/JackAudioMidiDriver.h`
- `src/backend/internal/jack/JackAudioMidiDriver.cpp`

The smart-pointer bridge helpers are in:

- `src/backend/internal/BridgeObject.h`
- `src/rust/backend_rust/src/rust_bridge_object.rs`

## Target architecture

The final Rust-side object should be the only decoupled MIDI port object:

```rust
pub struct DecoupledMidiPort {
    queue: DecoupledMidiQueue,
    direction: PortDirection,
    midi_port: cxx::UniquePtr<cpp_midi_port_cxx::ffi::MidiPortBridgeStrong>,
    maybe_driver: cxx::UniquePtr<audio_midi_driver_cxx::ffi::AudioMidiDriverBridgeWeak>,
    registry_handle: AtomicU64,
    closed: AtomicBool,
}
```

The exact module/type paths may differ, but the ownership model should match this shape.

The queue may either remain directly inside `DecoupledMidiPort` or be factored into an internal `DecoupledMidiQueue` helper. Factoring it out is recommended because the current Rust tests are queue-focused and can continue to test queue semantics without requiring a C++ `MidiPort` fixture.

C++ must not store or pass `std::shared_ptr<DecoupledMidiPort>` anymore. C++ must use Rust bridge-object handles:

- strong handle: `rust::Box<backend_rust::DecoupledMidiPortBridgeStrong>`
- weak handle: `rust::Box<backend_rust::DecoupledMidiPortBridgeWeak>`

The raw C API opaque `shoopdaloop_decoupled_midi_port_t*` should point to a heap-allocated Rust weak bridge handle, not a C++ `std::weak_ptr`.

`AudioMidiDriverCore` should store Rust decoupled-port registrations and call Rust `DecoupledMidiPort::process(nframes)` directly during `process_cycle`. It should no longer upgrade a C++ `DecoupledMidiPortBridgeWeak` and call C++ `PROC_process`.

## Phase 1: Add bridge prerequisites for C++ `MidiPort` and `AudioMidiDriver`

Add C++ bridge aliases for the underlying C++ types that the Rust decoupled port must own or reference.

In `src/backend/internal/MidiPort.h`, include `BridgeObject.h` if needed and add aliases after the `MidiPort` declaration:

```cpp
using MidiPortBridgeWeak = BridgeWeak<MidiPort>;
using MidiPortBridgeStrong = BridgeStrong<MidiPort>;
```

In `src/backend/internal/AudioMidiDriver.h`, add aliases after the `AudioMidiDriver` declaration:

```cpp
using AudioMidiDriverBridgeWeak = BridgeWeak<AudioMidiDriver>;
using AudioMidiDriverBridgeStrong = BridgeStrong<AudioMidiDriver>;
```

Create a Rust CXX bridge module for the C++ `MidiPort` object, for example `src/rust/backend_rust/src/cpp_midi_port_cxx.rs`. This bridge should expose:

- C++ opaque types `MidiPort`, `MidiPortBridgeStrong`, and `MidiPortBridgeWeak`.
- Bridge-object methods needed by Rust: `downgrade`, `upgrade`, `clone` for weak, `valid`, `get_ref`, and `get_pin_mut` where applicable.
- Safe-enough helper functions for the process path:
  - prepare a `MidiPort`
  - process a `MidiPort`
  - close a `MidiPort`
  - get the read-output MIDI buffer pointer for a frame count
  - get the write-into-port MIDI buffer pointer for a frame count
  - query a readable buffer's event count
  - copy one readable-buffer event into Rust out parameters
  - write one event into a writable buffer

Use raw pointer integers or raw pointers for buffer access at the bridge boundary rather than trying to expose C++ `MidiStorageElem` by value. The C++ `MidiStorageElem` is not the same type as the CXX shared Rust `MidiStorageElem`, so helper functions with explicit `time`, `size`, and `data` out parameters are less fragile.

A practical helper surface is:

```rust
unsafe fn midi_port_get_read_output_data_buffer(port: Pin<&mut MidiPort>, nframes: u32) -> usize;
unsafe fn midi_port_get_write_data_into_port_buffer(port: Pin<&mut MidiPort>, nframes: u32) -> usize;
unsafe fn midi_readable_buffer_n_events(buffer_ptr: usize) -> u32;
unsafe fn midi_readable_buffer_get_event(
    buffer_ptr: usize,
    idx: u32,
    out_time: &mut u32,
    out_size: &mut u16,
    out_data: *mut u8,
) -> bool;
unsafe fn midi_writeable_buffer_write_event(
    buffer_ptr: usize,
    time: u32,
    size: u16,
    data: *const u8,
) -> bool;
```

These helpers can be implemented as inline C++ functions in a small bridge-support header such as `src/backend/internal/MidiPortCxxBridge.h`, or added to an existing non-decoupled C++ header. They must not introduce a new C++ `DecoupledMidiPort` file.

Add bridge support for `AudioMidiDriverBridgeWeak/Strong` and for requesting closure by registry handle. The Rust decoupled port needs to be able to ask the owning driver to close/unregister a decoupled port without exposing C++ `DecoupledMidiPort`. Add a public C++ driver method like:

```cpp
void request_close_decoupled_midi_port(uint64_t registry_handle);
```

This method should queue a process-thread command that calls the Rust core's close/unregister path for that handle. It should capture a `shared_ptr<AudioMidiDriver>` internally if needed so the queued lambda cannot use a destroyed driver object. Do not capture move-only `rust::Box` values in `std::function` lambdas; use the stable registry handle instead.

Expose this request-close method to Rust through a CXX helper so `DecoupledMidiPort::request_close()` can upgrade its stored driver weak handle and call it.

Register any new Rust bridge modules in:

- `src/rust/backend_rust/src/lib.rs`
- `src/rust/backend_rust/build.rs`

Add `cargo:rerun-if-changed` entries for all new bridge files and helper headers.

## Phase 2: Replace the queue-only Rust type with the full Rust object

Refactor `src/rust/backend_rust/src/decoupled_midi_port.rs` so the queue behavior is preserved but the public `DecoupledMidiPort` becomes the full object.

Recommended internal split:

```rust
pub struct DecoupledMidiQueue {
    queue: ArrayQueue<MidiStorageElem>,
}

pub struct DecoupledMidiPort {
    queue: DecoupledMidiQueue,
    direction: PortDirection,
    midi_port: UniquePtr<MidiPortBridgeStrong>,
    maybe_driver: UniquePtr<AudioMidiDriverBridgeWeak>,
    registry_handle: AtomicU64,
    closed: AtomicBool,
}
```

Keep the queue semantics unchanged:

- bounded queue
- `push` returns `false` when full
- `pop` returns `None` when empty
- messages are silently dropped when full, matching existing behavior

Implement full-object methods in Rust:

- constructor from C++ `MidiPortBridgeStrong`, C++ `AudioMidiDriverBridgeWeak`, queue size, and direction
- `process(nframes)`
- `close()`
- `request_close()`
- `pop_incoming()`
- `push_outgoing()`
- `is_empty()`
- `direction()`
- `registry_handle()`
- `set_registry_handle(handle)`
- `mark_closed()` or equivalent
- accessor returning a fresh C++ `MidiPortBridgeStrong` or weak handle for C++ API code that still needs to call `PortInterface` methods on the underlying MIDI port

The Rust `process(nframes)` method must mirror the current C++ `DecoupledMidiPort::PROC_process` behavior.

For input ports:

1. Call `MidiPort::PROC_prepare(nframes)` through the bridge.
2. Call `MidiPort::PROC_process(nframes)` through the bridge.
3. Get `PROC_get_read_output_data_buffer(nframes)` through the bridge.
4. For each event in the readable buffer, push a `MidiStorageElem` into the queue, preserving the event time, size, and up to four data bytes.

For output ports:

1. Call `MidiPort::PROC_prepare(nframes)` through the bridge.
2. Get `PROC_get_write_data_into_port_buffer(nframes)` through the bridge.
3. Pop all queued messages and write them into the writable buffer.
4. Call `MidiPort::PROC_process(nframes)` through the bridge.

Handle null buffer pointers defensively. The current C++ code assumes non-null in the common path; the Rust port should avoid undefined behavior and simply skip reads/writes if a buffer pointer is null.

For wrong-direction operations, avoid panics across FFI. Prefer Rust methods that return `Result` internally and CXX wrapper functions that map errors into safe API behavior. If CXX `Result` is convenient in this project, use it to preserve the old C++ exception behavior for wrong-direction `pop_incoming`; otherwise return `false`/`None` and let the raw C API return its existing fallback value.

## Phase 3: Expose Rust bridge-object handles for `DecoupledMidiPort`

Use `define_rust_bridge_object_wrappers!` from `src/rust/backend_rust/src/rust_bridge_object.rs` to define Rust-native bridge wrappers:

```rust
define_rust_bridge_object_wrappers!(
    DecoupledMidiPortBridgeStrong,
    DecoupledMidiPortBridgeWeak,
    DecoupledMidiPort
);
```

Expose those wrappers through `src/rust/backend_rust/src/decoupled_midi_port_cxx.rs` or a renamed/expanded bridge module. The bridge should expose at least:

- constructor returning `Box<DecoupledMidiPortBridgeStrong>`
- strong methods: `id`, `valid`, `downgrade`, `clone_strong`, `strong_count`, `weak_count`
- weak methods: `id`, `valid`, `upgrade`, `clone`, `strong_count`, `weak_count`
- decoupled-port operations callable from C++ C API and driver code:
  - `decoupled_midi_port_process(strong, nframes)`
  - `decoupled_midi_port_request_close(strong)`
  - `decoupled_midi_port_close(strong)`
  - `decoupled_midi_port_pop_incoming(strong, out_time, out_size, out_data)`
  - `decoupled_midi_port_push_outgoing(strong, time, size, data)`
  - `decoupled_midi_port_registry_handle(strong)`
  - `decoupled_midi_port_set_registry_handle(strong, handle)`
  - `decoupled_midi_port_cpp_midi_port(strong)` returning a new C++ `MidiPortBridgeStrong` or weak handle

Do not keep `cpp_decoupled_midi_port_cxx.rs`, because that module exists only to call methods on the old C++ `DecoupledMidiPort` object.

The existing `decoupled_midi_port_bridge_cxx.rs` currently declares C++ bridge-object handles for the C++ object. Replace it with Rust-native bridge-wrapper declarations or remove it and consolidate into `decoupled_midi_port_cxx.rs`. The final names used by C++ should clearly refer to Rust-owned bridge wrappers, not C++ `BridgeStrong<DecoupledMidiPort>`.

## Phase 4: Update `AudioMidiDriverCore` and C++ audio drivers

Update `src/rust/backend_rust/src/audio_midi_driver.rs` so `DecoupledPortRegistration` stores Rust bridge handles instead of C++ bridge handles.

The registration should own a strong Rust decoupled-port handle to keep the port alive while it is registered. It should also store a weak handle or use the strong handle directly for processing.

`process_decoupled_port(handle, nframes)` should call the Rust port's `process(nframes)` directly. It must not call `get_pin_mut()` on a C++ `DecoupledMidiPort`, because that C++ type will no longer exist.

`close_decoupled_port(handle)` should call the Rust port's `close()` and mark the port closed. `unregister_decoupled_port(handle)` should remove the registration, dropping the registration strong handle. After unregistering, external raw C API handles should only hold weak handles and should fail to upgrade once no other strong handles remain.

Consider adding one combined Rust core method:

```rust
fn close_and_unregister_decoupled_port(self: &AudioMidiDriverCore, handle: u64) -> bool;
```

This avoids duplicated close/unregister sequencing and is the best target for the C++ driver's queued close request.

Update `src/rust/backend_rust/src/audio_midi_driver_cxx.rs` to use `Box<DecoupledMidiPortBridgeWeak>` and `Box<DecoupledMidiPortBridgeStrong>` for registration APIs instead of `UniquePtr` to C++ bridge objects.

Update `src/backend/internal/AudioMidiDriver.h/.cpp`:

- Change `make_decoupled_midi_port` to return `rust::Box<backend_rust::DecoupledMidiPortBridgeStrong>`.
- Change the virtual `open_decoupled_midi_port` return type similarly.
- Remove `unregister_decoupled_midi_port(std::shared_ptr<_DecoupledMidiPort>)`.
- Add handle-based close/unregister/request methods as needed.
- In `make_decoupled_midi_port`, create a `MidiPortBridgeStrong` from the C++ `std::shared_ptr<MidiPort>`, create an `AudioMidiDriverBridgeWeak` from `weak_driver_from_this()`, construct the Rust decoupled port, clone/downgrade it, register it with `AudioMidiDriverCore`, set its registry handle, and return the original strong handle.

Registration should produce a nonzero registry handle before `open_decoupled_midi_port` returns. Prefer `queue_and_wait` if preserving process-thread registry mutation is important. If registering directly through the Rust core's `RwLock` is acceptable, document why it is safe. Avoid the current asynchronous pattern where the handle may still be zero immediately after open, because the handle is now the stable close/unregister key.

Update derived drivers:

- `DummyAudioMidiDriver::open_decoupled_midi_port`
- `GenericJackAudioMidiDriver::open_decoupled_midi_port`

They should open a normal MIDI port as before, then call the updated `make_decoupled_midi_port` and return the Rust strong handle.

Remove override methods that only forwarded the old shared-pointer unregister call, or replace them with handle-based forwarding if still needed.

## Phase 5: Update the raw C API and test helpers

Replace the raw C API helper implementation in `src/backend/libshoopdaloop_backend.cpp`.

The old helpers are:

```cpp
shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(std::shared_ptr<_DecoupledMidiPort> port);
std::shared_ptr<_DecoupledMidiPort> internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port);
```

The new helpers should use heap-allocated Rust weak bridge handles:

```cpp
shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(
    rust::Box<backend_rust::DecoupledMidiPortBridgeStrong> const& strong);

rust::Box<backend_rust::DecoupledMidiPortBridgeStrong> internal_decoupled_midi_port(
    shoopdaloop_decoupled_midi_port_t *port);
```

`external_decoupled_midi_port` should allocate a weak handle produced by `strong->downgrade()` and reinterpret it as the opaque C type.

`internal_decoupled_midi_port` should cast the opaque pointer back to `rust::Box<backend_rust::DecoupledMidiPortBridgeWeak>*`, upgrade it, validate it, and throw `std::runtime_error` if the weak handle is invalid or expired. This preserves the existing invalid-handle behavior.

Update raw C API functions:

- `open_decoupled_midi_port` should call the updated driver method and create a C handle from the returned Rust strong handle.
- `maybe_next_message` should call the Rust `pop_incoming` wrapper and allocate a `shoop_midi_event_t` as before.
- `send_decoupled_midi` should call the Rust `push_outgoing` wrapper.
- `close_decoupled_midi_port` should upgrade the handle and call Rust `request_close`, which should route through the stored driver weak handle to queue close/unregister by registry handle.
- `get_decoupled_midi_port_name`, `get_decoupled_midi_port_connections_state`, `connect_external_decoupled_midi_port`, and `disconnect_external_decoupled_midi_port` can use `decoupled_midi_port_cpp_midi_port(strong)` to obtain the underlying C++ `MidiPort` bridge strong handle, then call the existing `PortInterface` methods on that C++ MIDI port. This preserves existing name pointer lifetime and connection-state behavior.
- `destroy_shoopdaloop_decoupled_midi_port` should delete the heap-allocated `rust::Box<DecoupledMidiPortBridgeWeak>` instead of leaking it. Any asynchronous close path must not depend on the public C handle pointer after returning from `close_decoupled_midi_port`; use the registry handle and/or cloned weak handles internally.

Update `src/backend/internal/libshoopdaloop_test_if.h` to remove references to `std::shared_ptr<_DecoupledMidiPort>` and expose the new handle helper signatures only if tests still need them.

Remove the `_DecoupledMidiPort = DecoupledMidiPort` alias and the `class DecoupledMidiPort` forward declaration from `src/backend/internal/shoop_globals.h` once no C++ code uses them.

## Phase 6: Delete obsolete C++ decoupled-port files and bridge files

Delete:

- `src/backend/internal/DecoupledMidiPort.h`
- `src/backend/internal/DecoupledMidiPort.cpp`
- `src/rust/backend_rust/src/cpp_decoupled_midi_port_cxx.rs`

Remove all includes of `DecoupledMidiPort.h`, especially from:

- `src/backend/libshoopdaloop_backend.cpp`
- `src/backend/internal/AudioMidiDriver.cpp`
- `src/backend/internal/AudioMidiDriverCxxTrampolines.h`
- C++ tests

Update `src/rust/backend_rust/src/lib.rs` and `src/rust/backend_rust/build.rs` to remove the deleted bridge module and rerun entries.

Because backend CMake uses `file(GLOB INTERNAL_SOURCES internal/*.cpp)`, deleting `DecoupledMidiPort.cpp` should automatically remove it from the backend library build. No explicit CMake source-list removal should be required unless another CMake file references it directly.

## Phase 7: Update tests

Update C++ tests that currently construct or inspect C++ `DecoupledMidiPort` shared pointers.

In `src/backend/test/unit/test_BridgeObject.cpp`, remove the tests that verify `BridgeStrong<DecoupledMidiPort>` over C++ `std::shared_ptr`. Replace them with either:

- tests for `MidiPortBridgeStrong/Weak`, because that C++ bridge object is now the object Rust owns, and/or
- C++ tests that create a Rust decoupled port through the new Rust constructor and verify Rust strong/weak bridge behavior.

In `src/backend/test/unit/test_DummyAudioMidiDriver.cpp`, update decoupled-port tests to use Rust bridge handles:

- open returns a Rust strong handle, not `std::shared_ptr`.
- keepalive tests should use Rust weak handle validity/upgrade and strong/weak counts rather than `std::weak_ptr`.
- close/unregister should be requested by registry handle through the driver/core path.

Keep and update integration tests in `src/backend/test/integration/test_libshoopdaloop_if.cpp` that exercise the raw C API:

- open decoupled MIDI port
- send decoupled MIDI
- receive maybe-next-message for input ports
- name lookup
- connection state
- close
- destroy handle

Add Rust unit tests for the queue helper if it is factored out. Add Rust tests for the Rust bridge object wrappers if the public wrapper methods are nontrivial.

## Safety and lifecycle notes

Do not pass C++ references or raw buffer pointers out of their valid process scope. The Rust `process(nframes)` method may temporarily use raw buffer pointers returned by C++ helpers, but those pointers must not be stored in the Rust object.

Do not let panics cross CXX. All CXX-exposed Rust functions should return safe fallback values or `Result` where appropriate.

Do not capture move-only `rust::Box` values in `std::function<void()>` process-thread command lambdas. Use registry handles, copyable `std::shared_ptr` wrappers around cloned handles, or Rust core methods that can be called by handle.

Preserve real-time behavior as much as the current implementation does. The current Rust `AudioMidiDriverCore` already uses locks for registration maps; avoid adding new heap allocations or locks inside per-message processing beyond what the existing queue and C++ MIDI buffers already do.

The C API `get_decoupled_midi_port_name` returns a `const char*`. Prefer returning the underlying C++ `PortInterface::name()` pointer through the bridged MIDI port rather than returning a pointer to a temporary Rust `String`.

## Build and test instructions

The project build is managed by Cargo.

During development, build with:

```bash
cargo build
```

Run Rust tests with:

```bash
cargo test
```

The C++ Catch2 tests are built as a side effect of building the backend crate. After `cargo build`, locate and run the generated test runner, for example:

```bash
find target -name test_runner -type f -executable
```

Then run the discovered executable. If multiple `test_runner` binaries exist, use the one under the current `target/debug` backend build output.

For final validation, format and build with warnings denied:

```bash
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
cargo test
```

If relevant for end-to-end validation, also run the QML self-tests after building:

```bash
./target/debug/shoopdaloop_dev.sh --self-test
```

If a dependency outside the project fails to build because the environment is not set up, report that and stop rather than trying to vendor or rewrite dependencies.
