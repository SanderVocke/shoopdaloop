# Plan: make Rust `AudioMidiDriverCore` the single source of truth for processor membership

## Goal

Remove the C++ compatibility mirror list of processors currently stored in `AudioMidiDriver::m_processors`. `AudioMidiDriverCore` already owns the authoritative processor registration records, including bridge strong handles for keepalive and bridge weak handles for processing. The remaining C++ vector duplicates membership state only to implement the legacy C++ API method:

```cpp
std::vector<std::weak_ptr<HasAudioProcessingFunction>> AudioMidiDriver::processors() const;
```

The new design should keep that public API method available, but implement it by querying Rust-owned processor registrations and reconstructing a C++ `std::vector<std::weak_ptr<HasAudioProcessingFunction>>` on demand.

The end state should be:

- `AudioMidiDriver` has no `m_processors` member.
- `AudioMidiDriver::add_processor` registers only with the bridge registry and Rust core; it does not update a C++ processor list.
- `AudioMidiDriver::remove_processor` removes only from Rust core using C++ identity; it does not update a C++ processor list.
- `AudioMidiDriver::processors()` asks Rust for the currently registered processor bridge weak handles, resolves them via the existing typed bridge resolver pattern, and returns weak pointers to successfully resolved C++ processors.
- Rust `AudioMidiDriverCore` remains the single source of truth for processor membership and keepalive ownership.
- Existing process-cycle behavior remains unchanged: Rust processes processors from its registration records using bridge weak handles and typed resolver helpers.
- Existing public C++ API behavior remains stable for any current or future caller of `processors()`.

## Relevant code

Primary files:

- `src/backend/internal/AudioMidiDriver.h`
- `src/backend/internal/AudioMidiDriver.cpp`
- `src/rust/backend_rust/src/audio_midi_driver.rs`
- `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`

Bridge object files:

- `src/backend/internal/BridgeObject.h`
- `src/backend/internal/BridgeObject.cpp`
- `src/rust/backend_rust/src/bridge_object_cxx.rs`

Concrete driver forwarding declarations/implementations, likely unchanged except for compile fallout:

- `src/backend/internal/DummyAudioMidiDriver.h/.cpp`
- `src/backend/internal/jack/JackAudioMidiDriver.h/.cpp`

Tests likely affected:

- `src/backend/test/unit/test_DummyAudioMidiDriver.cpp`
- `src/backend/test/unit/test_BridgeObject.cpp`
- backend integration tests that add/remove processors

## Current behavior

`AudioMidiDriver::add_processor` currently does two things:

1. pushes the processor into a C++ vector of weak pointers, `m_processors`,
2. registers a bridge object and passes the weak/strong handle scalar fields into `AudioMidiDriverCore`.

`AudioMidiDriver::remove_processor` currently does two things:

1. erases the processor from `m_processors`,
2. tells `AudioMidiDriverCore` to remove the registration by C++ identity.

`AudioMidiDriver::processors()` currently returns `m_processors`.

The Rust side already contains processor registration records similar to:

```rust
struct ProcessorRegistration {
    cpp_identity: usize,
    weak: BridgeWeakHandle,
    strong: BridgeStrongHandle,
}
```

Those records are the authoritative process-thread data. The C++ mirror list is only a compatibility cache.

## Design constraints

Keep the public C++ virtual API stable. Do not remove or change the signature of:

```cpp
virtual std::vector<std::weak_ptr<HasAudioProcessingFunction>> processors() const;
```

Do not make processors Rust-owned. Processors remain C++ objects registered through bridge handles.

Do not add a second lifetime-authoritative C++ collection.

Do not reintroduce C++ strong-handle maps for processors.

Do not use raw C++ object pointers for processing or lifetime. Raw C++ pointer identity may remain in Rust only as an identity key for removal.

Keep using the bridge-object typed resolver pattern:

- Rust stores bridge weak handles.
- C++ or Rust resolves weak handles through typed bridge helpers.
- C++ operations receive typed `std::shared_ptr<HasAudioProcessingFunction>`.

`processors()` is not a realtime/process-thread path, so it may allocate and briefly lock the bridge registry.

## CXX bridge type strategy

Avoid trying to return `crate::bridge_object_cxx::ffi::BridgeWeakHandle` directly from `audio_midi_driver_cxx.rs`. CXX bridge types are module-scoped, and reusing a struct defined in another bridge module can cause type identity or unsupported-type issues.

Instead, define a small local POD struct in `audio_midi_driver_cxx.rs`, for example:

```rust
#[derive(Clone, Copy, Debug)]
struct AudioMidiDriverProcessorWeakHandle {
    id: u64,
    type_id: u32,
}
```

Expose a Rust method such as:

```rust
fn get_processor_weak_handles(self: &AudioMidiDriverCore) -> Vec<AudioMidiDriverProcessorWeakHandle>;
```

The implementation can convert from internal `BridgeWeakHandle` values to this CXX-local POD struct.

Alternative if CXX has trouble with a vector of local structs: expose parallel scalar vectors or a count/index API. Prefer the local POD struct first because it is clearer and keeps call sites simple.

## Implementation phases

### Phase 1: expose Rust-owned processor weak handles to C++

Add a method to `AudioMidiDriverCore` that returns a snapshot of processor weak handles from the Rust registration map. The method should read the processor map, collect `reg.weak` for each registration, and convert to the CXX-local handle struct used by `audio_midi_driver_cxx.rs`.

The snapshot order does not need to be stable unless existing tests assume it. Current processor maps are hash maps, so order should not be treated as meaningful.

Expose the method through `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`.

### Phase 2: implement `AudioMidiDriver::processors()` from Rust handles

Remove `m_processors` from `AudioMidiDriver` in `AudioMidiDriver.h`.

Change `AudioMidiDriver::add_processor` so it no longer pushes into `m_processors`. It should continue to:

1. call `bridge_object::register_processor(p)`,
2. downgrade the strong handle,
3. call Rust core `add_processor(cpp_identity, weak.id, weak.type_id, strong.id, strong.type_id)`.

Change `AudioMidiDriver::remove_processor` so it no longer erases from `m_processors`. It should continue to call Rust core removal by C++ identity.

Change `AudioMidiDriver::processors()` to:

1. call `m_rust_core->get_processor_weak_handles()`,
2. for each returned weak handle, resolve the processor using the existing C++ typed bridge resolver, e.g. `bridge_object::bridge_resolve_processor_for_rust(id, type_id)` or `bridge_object::lock_processor(BridgeWeakHandle{id, type_id})`,
3. if resolution succeeds, push a `std::weak_ptr<HasAudioProcessingFunction>` into the return vector,
4. skip stale/unresolvable handles defensively.

Because Rust owns strong handles for registered processors, live registrations should normally resolve. Skipping stale handles makes the API robust if cleanup races or stale handles are encountered.

### Phase 3: tests and validation

Add or update tests if needed to cover `processors()` behavior now that it is reconstructed from Rust state.

Useful test scenarios:

- A new driver has an empty `processors()` vector.
- After adding a processor, `processors()` returns one weak pointer that locks to the same C++ object.
- After removing that processor, `processors()` returns empty or no longer includes that object.
- Adding multiple processors returns all live objects, without assuming order.
- Destroyed/stale registrations are not returned as lockable weak pointers, if such a scenario can be represented safely.

Potential test location:

- `src/backend/test/unit/test_DummyAudioMidiDriver.cpp`

If existing tests already cover sufficient add/remove behavior, update them to assert `processors()` as described.

### Phase 4: cleanup and final verification

Search and confirm:

- no `m_processors` member remains in `AudioMidiDriver`,
- no C++ processor membership vector/list remains as a mirror of Rust processor registrations,
- `AudioMidiDriver::processors()` reconstructs from Rust-owned weak handles,
- Rust still owns processor strong handles,
- no C++ processor bridge strong keepalive map was introduced,
- process-cycle code still uses Rust processor registrations and typed resolver helpers.

Run formatting and full validation.

## Build and test instructions

Project build is Cargo-managed from repository root.

Iterative build:

```bash
cargo build
```

Final strict build:

```bash
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

Rust tests:

```bash
cargo test
```

Backend C++ tests:

```bash
find target -type f -name test_runner
# then run the located executable, for example:
target/debug/build/.../out/cmake_install/tools/shoopdaloop/test_runner
```

QML/application self-test:

```bash
./target/debug/shoopdaloop_dev.sh --self-test
```

If Qt platform initialization fails in the current environment, retry the self-test with:

```bash
QT_QPA_PLATFORM=offscreen ./target/debug/shoopdaloop_dev.sh --self-test
```

If dependency/environment failures occur outside project code, report the logs and stop. Otherwise fix project warnings/errors introduced by this task.
