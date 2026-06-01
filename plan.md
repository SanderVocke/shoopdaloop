# Plan: introduce bridge strong/weak handles for processors and decoupled MIDI ports

## Current Progress Snapshot

Completed:

- Bridge object infrastructure scaffold added:
  - `src/backend/internal/BridgeObject.h/.cpp`
  - `src/rust/backend_rust/src/bridge_object_cxx.rs`
- Processor family (Milestone 1) migrated from raw pointer registration to bridge weak-handle dispatch.
- `AudioMidiDriverCore` processor storage no longer uses raw `usize` pointers.
- C++ processor trampoline now upgrades/locks processor bridge weak handles before dispatch.
- Processor lifecycle coverage added in unit tests:
  - `DummyAudioMidiDriver - processor add/remove cycles`.
- Validation completed for Milestone 1:
  - `cargo build`
  - `cargo test`
  - backend `test_runner`

Still pending:

- Documenting explicit real-time/process-thread policy limits of the first mutex-based bridge registry implementation.
- Milestone 2 decoupled MIDI bridge-handle migration and its validation.
- Final cleanup and strict verification gate.

## Goal

Introduce the first reusable bridge ownership mechanism for the C++ to Rust migration. The immediate goal is not to remove all raw pointers everywhere. The goal is to establish a typed strong/weak handle system that preserves current C++ `shoop_shared_ptr` / `shoop_weak_ptr` keepalive semantics across the Rust/C++ boundary, then apply it to two object families:

1. audio processing callback objects (`HasAudioProcessingFunction` processors)
2. decoupled MIDI ports (`DecoupledMidiPort`)

At the end of these milestones, Rust `AudioMidiDriverCore` should not store raw C++ pointers for these two families as lifetime-authoritative references. Instead it should store bridge handles and use C++ trampolines that upgrade/resolve those handles safely.

This plan implements the first concrete steps of the long-term migration vision in `porting.md`.

## Current relevant code

Core driver/runtime code:

- `src/backend/internal/AudioMidiDriver.h`
- `src/backend/internal/AudioMidiDriver.cpp`
- `src/backend/internal/AudioMidiDriverRuntime.h`
- `src/backend/internal/AudioMidiDriverRuntime.cpp`
- `src/backend/internal/AudioMidiDriverCxxTrampolines.h`
- `src/rust/backend_rust/src/audio_midi_driver.rs`
- `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`

Processor type:

- `HasAudioProcessingFunction` is declared in `src/backend/internal/AudioMidiDriver.h`.
- Processor registration currently goes through `AudioMidiDriverRuntime::add_processor/remove_processor`.

Decoupled MIDI code:

- `src/backend/internal/DecoupledMidiPort.h`
- `src/backend/internal/DecoupledMidiPort.cpp`
- `src/rust/backend_rust/src/decoupled_midi_port.rs`
- `src/rust/backend_rust/src/decoupled_midi_port_cxx.rs`
- Current decoupled registration is in `AudioMidiDriverRuntime` and `AudioMidiDriverCore`.

Factory/API/test areas likely touched:

- `src/backend/libshoopdaloop_backend.cpp`
- `src/backend/test/unit/test_DummyAudioMidiDriver.cpp`
- `src/backend/test/integration/test_libshoopdaloop_if.cpp`

## Design constraints

Keep the public C API and existing C++ driver interface behavior stable.

Do not attempt to solve Rust-owned objects in the first version. The initial bridge registry may be C++-owned and wrap existing C++ `shoop_shared_ptr` objects.

Do not expose C++ templates through CXX. Expose non-template handle structs and non-template bridge functions. Add typed convenience wrappers only on the C++ side.

Preserve current process-cycle behavior and ordering:

- maybe-process callback
- command queue execution
- decoupled MIDI port processing
- processor processing
- last-processed state update

Keep audio-thread impact documented. The first version may use mutex-protected registries if needed, but avoid adding unnecessary allocation or blocking to hot paths after registration.

Add tests for stale handles and unregister/destroy ordering.

## Milestone 1: bridge handles for processors

### Intended behavior

Current Rust `AudioMidiDriverCore` stores processor raw pointers as `usize`:

```rust
processors: RwLock<HashMap<u64, usize>>
```

This should become bridge-handle based, for example:

```rust
processors: RwLock<HashMap<u64, BridgeWeakHandle>>
```

During process-cycle dispatch, Rust should call a C++ trampoline with a weak handle. C++ should attempt to upgrade/resolve that weak handle. If it succeeds, it calls:

```cpp
processor->PROC_process(nframes);
```

If it fails, it should skip safely.

### C++ bridge object infrastructure

Add a new small bridge ownership module under `src/backend/internal/`, for example:

- `BridgeObject.h`
- `BridgeObject.cpp`

The first implementation can be C++-owned and non-template internally. It should define CXX-safe POD-ish handle structs, either in a CXX bridge or C++ header included by a CXX bridge:

```cpp
struct BridgeStrongHandle {
    uint64_t id = 0;
    uint32_t type_id = 0;
};

struct BridgeWeakHandle {
    uint64_t id = 0;
    uint32_t type_id = 0;
};
```

Use explicit type IDs for object families, at least:

- processor / `HasAudioProcessingFunction`
- decoupled MIDI port / `DecoupledMidiPort`

The C++ side should provide typed wrappers or helper functions. For processors, it is enough to support:

- register a strong C++ `shoop_shared_ptr<HasAudioProcessingFunction>` and return a strong bridge handle
- downgrade a strong bridge handle to a weak bridge handle
- release a strong bridge handle when it is no longer needed
- upgrade/resolve a weak handle for the duration of a trampoline call

A simple implementation may use a global or singleton registry protected by a mutex:

- `id -> type_id`
- `id -> shoop_weak_ptr<void-like-record>` or typed erasure record
- strong records held by `BridgeStrongHandle` wrappers / registry ref counts

Because `std::shared_ptr<void>` can hold a typed shared pointer while preserving the deleter, the non-template registry can store erased `std::shared_ptr<void>` plus a `type_id`. If `USE_TRACKED_SHARED_PTRS` complicates direct erasure, use a typed holder base class:

```cpp
struct BridgeHolderBase { virtual ~BridgeHolderBase() = default; };
template<typename T> struct BridgeHolder : BridgeHolderBase { shoop_shared_ptr<T> ptr; };
```

Then the registry stores `std::shared_ptr<BridgeHolderBase>` and returns typed pointers through helper functions that validate `type_id` and `dynamic_cast` to the expected holder.

### CXX bridge for handle structs

Create or extend a Rust/CXX bridge so Rust can store these handles. Possible file names:

- `src/rust/backend_rust/src/bridge_object.rs`
- `src/rust/backend_rust/src/bridge_object_cxx.rs`

Expose the handle structs to Rust. Keep them cheap to clone/copy.

If CXX has trouble with shared struct definitions in C++ headers, define the structs inside the CXX bridge and generate the header, then include that generated header from C++ files that need the handles.

### Processor registration flow

Change `AudioMidiDriverRuntime::add_processor` from raw pointer registration to bridge registration.

Current pattern:

```cpp
m_processors.push_back(p);
auto handle = m_rust_core->add_processor(reinterpret_cast<uintptr_t>(p.get()));
m_processor_handles[p.get()] = handle;
```

Target pattern:

```cpp
m_processors.push_back(p);
auto strong = bridge_register_processor(p);
auto weak = bridge_downgrade(strong);
auto handle = m_rust_core->add_processor(weak);
m_processor_handles[p.get()] = {handle, strong};
```

The exact C++ storage can vary. The important points are:

- `m_processors` may remain as a compatibility list for the public `processors()` API.
- Rust receives a weak handle, not a raw pointer.
- C++ keeps enough registration state to unregister and release any strong registration handle cleanly.
- Removing a processor unregisters the Rust handle and releases the bridge strong handle.

Alternatively, if registering a processor should not itself keep the processor alive, store only weak handles in C++ and Rust. However, the existing runtime stores weak pointers in `m_processors` but Rust stores raw pointers; the practical lifetime today depends on external owners. Preserve that intent unless tests indicate registered processors must be strongly kept alive. Document the chosen behavior.

### Rust `AudioMidiDriverCore` changes for processors

Update `audio_midi_driver.rs`:

- introduce/use `BridgeWeakHandle`
- replace processor `usize` storage with bridge weak handles
- update `add_processor` signature in Rust to accept a weak handle
- update CXX bridge declaration in `audio_midi_driver_cxx.rs`
- update processing loop to dispatch bridge weak handles

The C++ trampoline should change from:

```cpp
void audiomididriver_process_processor(uintptr_t processor_ptr, uint32_t nframes);
```

to something like:

```cpp
void audiomididriver_process_processor_bridge(BridgeWeakHandle processor, uint32_t nframes);
```

or update the existing function signature if practical. The trampoline should upgrade/resolve the weak handle and call `PROC_process` only if valid.

### Processor tests

Add or update tests to cover:

- normal processor registration and process dispatch still works
- removing a processor prevents further dispatch
- destroying a processor after unregister does not leave a stale raw pointer in Rust
- attempting to process a stale weak handle is safe/no-op
- repeated add/remove cycles under dummy controlled processing

Existing `test_DummyAudioMidiDriver.cpp` is a good place for unit coverage.

## Milestone 2: bridge handles for decoupled MIDI ports

### Intended behavior

Current Rust `AudioMidiDriverCore` stores decoupled MIDI port raw pointers as `usize` and C++ `AudioMidiDriverRuntime` keeps registered ports alive in a map:

```cpp
std::unordered_map<uint64_t, shoop_shared_ptr<DecoupledMidiPort>> m_decoupled_midi_ports;
```

Replace the raw pointer registry with bridge handles. The runtime should register decoupled MIDI ports through the bridge object system and Rust should store handles instead of raw pointers.

A registered decoupled MIDI port should be intentionally kept alive while registered, but that keepalive should be represented by a bridge strong handle rather than an ad hoc map of C++ shared pointers.

### Decoupled registration flow

Current pattern:

```cpp
auto decoupled = shoop_make_shared<DecoupledMidiPort>(...);
rust_command_queue::queue(*m_command_queue, [this, decoupled]() {
    auto handle = m_rust_core->register_decoupled_port(reinterpret_cast<uintptr_t>(decoupled.get()));
    decoupled->set_registry_handle(handle);
    m_decoupled_midi_ports[handle] = decoupled;
});
```

Target pattern:

```cpp
auto decoupled = shoop_make_shared<DecoupledMidiPort>(...);
auto strong = bridge_register_decoupled_midi_port(decoupled);
auto weak = bridge_downgrade(strong);
rust_command_queue::queue(*m_command_queue, [this, decoupled, strong, weak]() mutable {
    auto handle = m_rust_core->register_decoupled_port(weak);
    decoupled->set_registry_handle(handle);
    m_decoupled_midi_port_handles[handle] = std::move(strong);
});
```

The C++ runtime may still need a map, but it should be a map of bridge strong handles rather than direct `shared_ptr<DecoupledMidiPort>`. This is the intermediate win: keepalive becomes explicit bridge infrastructure and can later move to Rust.

### Rust `AudioMidiDriverCore` changes for decoupled ports

Update decoupled storage in `audio_midi_driver.rs` from raw pointer values to bridge weak handles or strong handles depending on the chosen keepalive policy.

Suggested policy for this milestone:

- Rust stores weak handles for process dispatch.
- C++ runtime stores strong bridge handles while registered.
- Unregister removes the Rust weak handle and releases the C++ runtime strong bridge handle.

Update methods:

- `register_decoupled_port`
- `unregister_decoupled_port`
- `process_decoupled_port`
- `close_decoupled_port`
- `get_decoupled_ports` or replace it with handle-oriented accessors
- `process_cycle`

### Decoupled trampolines

Change trampolines from raw pointer dispatch:

```cpp
void audiomididriver_process_decoupled_port(uintptr_t decoupled_port_ptr, uint32_t nframes);
void audiomididriver_close_decoupled_port(uintptr_t decoupled_port_ptr);
```

to bridge handle dispatch, for example:

```cpp
void audiomididriver_process_decoupled_port_bridge(BridgeWeakHandle port, uint32_t nframes);
void audiomididriver_close_decoupled_port_bridge(BridgeWeakHandle port);
```

The C++ implementation should upgrade/resolve the handle and call:

```cpp
port->PROC_process(nframes);
port->close();
```

only if the handle is valid.

### Decoupled C++ runtime storage

Replace or rename:

```cpp
std::unordered_map<uint64_t, shoop_shared_ptr<shoop_types::_DecoupledMidiPort>> m_decoupled_midi_ports;
```

with a map that stores bridge strong handles or a small registration record:

```cpp
struct RegisteredDecoupledPort {
    BridgeStrongHandle strong;
    BridgeWeakHandle weak;
};
std::unordered_map<uint64_t, RegisteredDecoupledPort> m_decoupled_midi_ports;
```

The direct `shared_ptr` should not be needed for keepalive after registration.

### Decoupled tests

Add or update tests for:

- normal decoupled MIDI send/receive behavior still works
- close/unregister during active dummy processing remains safe
- repeated open/close cycles do not crash
- stale decoupled handle operations fail safely
- registered decoupled port remains alive until unregister even if local C++ shared pointer is dropped, if that is the intended behavior

Existing tests in `test_DummyAudioMidiDriver.cpp` and `test_libshoopdaloop_if.cpp` are likely the right places.

## Cleanup expectations

After both milestones:

- Rust `AudioMidiDriverCore` no longer stores processor or decoupled MIDI raw object pointers as `usize`.
- C++ `AudioMidiDriverRuntime` no longer has ad hoc direct `shared_ptr` keepalive maps for decoupled MIDI ports.
- C++ trampolines resolve bridge handles safely before invoking C++ object methods.
- Stale weak handles are safe no-ops.
- The bridge object module is documented as migration infrastructure.

Avoid expanding the bridge system to unrelated object families in this task.

## Build and test instructions

Project-specific build/test guidance comes from `AGENTS.md`, `.agents/build.md`, and `.agents/test.md`.

Development commands from the repository root:

- Iterative build: `cargo build`
- Rust tests: `cargo test`
- Backend C++ tests: locate the Catch2 runner with `find target -type f -name test_runner`, then run the appropriate `target/.../test_runner`
- QML/application self-test: `./target/debug/shoopdaloop_dev.sh --self-test`

Recommended milestone gates:

- After adding bridge infrastructure but before changing driver code: `cargo build`
- After processor migration: `cargo build`, `cargo test`, backend `test_runner`
- After decoupled MIDI migration: `cargo build`, `cargo test`, backend `test_runner`, self-test

Final required gate:

- `cargo fmt --all`
- `RUSTFLAGS="-D warnings" cargo build`
- `cargo test`
- backend `test_runner`
- `./target/debug/shoopdaloop_dev.sh --self-test`

If external dependency or environment setup issues occur outside project code, report them with logs and stop. Otherwise, fix project errors and warnings introduced by the task.
