# Plan: remove `AudioMidiDriverRuntime` by moving its responsibilities into Rust `AudioMidiDriverCore`

## Goal

Remove the C++ `AudioMidiDriverRuntime` class and make `AudioMidiDriverCore` the canonical owner of the runtime state that is currently split between C++ and Rust. The C++ `AudioMidiDriver` virtual interface and concrete C++ drivers should remain for now. C++ callers/drivers may keep using virtual dispatch, but the internal runtime bookkeeping, command queue ownership, and bridge strong-handle keepalive records should move into Rust where practical.

The end state should be:

- `src/backend/internal/AudioMidiDriverRuntime.h/.cpp` removed or left as obsolete-empty only if build constraints require a short transition, with no runtime functionality remaining there.
- `AudioMidiDriverCore` owns the command queue and process-cycle runtime data.
- `AudioMidiDriverCore` owns processor bridge registration records, including strong handles for keepalive and weak handles for processing.
- `AudioMidiDriverCore` owns decoupled MIDI bridge registration records, including strong handles for keepalive and weak handles for processing.
- C++ `AudioMidiDriver` and concrete C++ drivers remain as the virtual interface layer, delegating directly to Rust core and/or small C++ free-function helpers where Rust cannot directly construct or receive C++ objects.
- Existing public C API and C++ API behavior remains stable.
- Existing bridge-object typed resolver pattern remains in use for processors and decoupled MIDI ports.

## Current relevant code

Core runtime/driver files:

- `src/backend/internal/AudioMidiDriver.h`
- `src/backend/internal/AudioMidiDriver.cpp`
- `src/backend/internal/AudioMidiDriverRuntime.h`
- `src/backend/internal/AudioMidiDriverRuntime.cpp`
- `src/backend/internal/AudioMidiDriverCxxTrampolines.h`
- `src/backend/internal/DummyAudioMidiDriver.h/.cpp`
- `src/backend/internal/jack/JackAudioMidiDriver.h/.cpp`
- `src/rust/backend_rust/src/audio_midi_driver.rs`
- `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`

Bridge object files:

- `src/backend/internal/BridgeObject.h/.cpp`
- `src/rust/backend_rust/src/bridge_object_cxx.rs`

Command queue files:

- `src/backend/internal/RustCommandQueue.h`
- `src/rust/backend_rust/src/command_queue.rs`
- `src/rust/backend_rust/src/command_queue_cxx.rs`

Decoupled MIDI files:

- `src/backend/internal/DecoupledMidiPort.h/.cpp`
- `src/rust/backend_rust/src/decoupled_midi_port.rs`
- `src/rust/backend_rust/src/decoupled_midi_port_cxx.rs`

Tests likely affected:

- `src/backend/test/unit/test_DummyAudioMidiDriver.cpp`
- `src/backend/test/unit/test_BridgeObject.cpp`
- `src/backend/test/integration/test_libshoopdaloop_if.cpp`
- Chain/integration tests using dummy/JACK drivers under `src/backend/test/integration/`

## Existing `AudioMidiDriverRuntime` responsibilities to migrate

`AudioMidiDriverRuntime` currently owns or provides:

- `rust::Box<backend_rust::AudioMidiDriverCore> m_rust_core`
- `rust::Box<backend_rust::CommandQueue> m_command_queue`
- `std::vector<std::weak_ptr<HasAudioProcessingFunction>> m_processors` for public `processors()` compatibility
- processor registration maps:
  - C++ object pointer to Rust processor handle
  - C++ object pointer to bridge strong handle
- decoupled MIDI registration map:
  - Rust registry handle to bridge strong/weak registration record
- maybe-process callback pointer
- client name cache for `const char*` return compatibility
- state delegation to Rust core
- command-queue enqueue/execute/wait helpers
- process cycle invocation
- decoupled MIDI construction and unregister orchestration

The migration should remove this C++ owner object by moving ownership/data into `AudioMidiDriverCore` and replacing runtime methods with either direct Rust core methods or small C++ free functions.

## Design constraints

Keep the current public C API and C++ virtual `AudioMidiDriver` interface stable.

Do not migrate concrete drivers (`DummyAudioMidiDriver`, JACK drivers) to Rust as part of this work.

Do not make processors or decoupled MIDI ports Rust-owned. They should remain C++ objects registered through bridge handles.

Do not reintroduce raw object-pointer storage as lifetime-authoritative state in Rust. Raw C++ object addresses may only be used as lookup keys or transient identity keys where unavoidable.

Preserve process-cycle ordering:

1. maybe-process callback
2. command queue execution
3. decoupled MIDI port processing
4. processor processing
5. last-processed state update

Keep the existing bridge-object typed resolver pattern for processor and decoupled MIDI calls:

- Rust stores bridge weak handles.
- Rust resolves to typed CXX `SharedPtr<T>` via typed bridge resolver.
- Rust calls typed operation helper with the resolved pointer.

Avoid adding unnecessary process-thread allocation or blocking beyond what already exists. Current bridge registry locking limitations are accepted, but do not make them worse without documenting it.

## Proposed target architecture

### Rust `AudioMidiDriverCore`

Expand `AudioMidiDriverCore` to own runtime records currently held in C++:

- command queue
- maybe-process callback pointer, or keep passed in from C++ if pointer lifetime concerns make that simpler initially
- processor registrations with both bridge strong and bridge weak handles
- decoupled port registrations with both bridge strong and bridge weak handles
- compatibility list or data necessary to keep `processors()` behavior stable on C++ side

Suggested Rust structures:

```rust
struct ProcessorRegistration {
    weak: BridgeWeakHandle,
    strong: BridgeStrongHandle,
    cpp_identity: usize,
}

struct DecoupledPortRegistration {
    weak: BridgeWeakHandle,
    strong: BridgeStrongHandle,
}
```

`AudioMidiDriverCore` can keep maps such as:

```rust
processors: RwLock<HashMap<u64, ProcessorRegistration>>,
processor_handle_by_cpp_identity: Mutex<HashMap<usize, u64>>,
decoupled_ports: RwLock<HashMap<u64, DecoupledPortRegistration>>,
command_queue: CommandQueue,
maybe_process_callback_ptr: AtomicUsize,
```

The exact representation can differ, but Rust should own the strong-handle release responsibility after registration.

### C++ free-function helpers

Rust/CXX cannot directly receive arbitrary `std::shared_ptr<HasAudioProcessingFunction>` or construct C++ `DecoupledMidiPort` objects without typed help. Replace `AudioMidiDriverRuntime` methods with small C++ helpers, for example:

- register a processor bridge object and call Rust core with scalar handle fields
- unregister/release processor records by Rust handle or C++ identity
- create decoupled MIDI ports and register bridge object handles with Rust core

These helpers should not own long-lived state. They should be narrow adapters around C++ object construction or bridge registration.

### C++ driver classes

Concrete drivers should hold the Rust core directly rather than holding `AudioMidiDriverRuntime`, for example:

```cpp
rust::Box<backend_rust::AudioMidiDriverCore> m_rust_core;
```

Or the C++ base class `AudioMidiDriver` may own a protected core if that is simpler and compatible with existing inheritance. The selected placement should minimize duplication across Dummy and JACK drivers.

Virtual methods continue to exist and delegate to Rust/core/helper functions.

## Phase 1: move command queue ownership into Rust core

Add a `CommandQueue` field to `AudioMidiDriverCore`.

Update Rust `process_cycle` so it no longer receives `command_queue_ptr`. It should execute the owned queue directly after invoking the maybe-process callback.

Add Rust/CXX methods on `AudioMidiDriverCore` for:

- queuing a process-thread command from C++
- queue-and-wait / execute process-thread command from C++ if the current API requires it
- executing all commands for process thread

Because C++ commands are currently `std::function<void()>`, this may still need existing `RustCommandQueue.h` helper functions and CXX support. If fully moving command queue ownership proves too large, keep a tiny C++ helper function that enqueues into the Rust-owned queue, but do not keep `AudioMidiDriverRuntime` as the owner.

Update C++ drivers to use the Rust core command-queue APIs directly.

Milestone validation: `cargo build`, then backend `test_runner` if build changes are non-trivial.

## Phase 2: move processor registration bookkeeping into Rust core

Change Rust processor registration APIs so Rust receives and stores:

- C++ identity pointer as `usize` only for lookup/removal identity, not for processing/lifetime
- bridge strong handle fields
- bridge weak handle fields

Rust should allocate the stable processor handle and store a `ProcessorRegistration` containing both handles.

Rust removal by C++ identity or stable handle should:

- remove the registration from Rust maps
- call Rust-side bridge `release_strong` for the stored strong handle, which routes to the C++ registry for C++-owned handles

C++ helper for processor add should:

- accept `std::shared_ptr<HasAudioProcessingFunction>`
- call `bridge_object::register_processor`
- downgrade it
- pass scalar fields and C++ identity to Rust core

C++ helper for processor removal should:

- pass C++ identity to Rust core
- not own/release the bridge strong handle itself

Preserve `processors()` compatibility behavior. Either keep the weak-list in the C++ concrete driver/base class, or move enough state to Rust and reconstruct a compatible vector. Since `processors()` returns C++ weak pointers, it is acceptable for the C++ virtual layer to keep only this compatibility vector while Rust owns actual registration bookkeeping.

Milestone validation: add/update tests around processor add/remove cycles and stale processor handle safety, then run `cargo build` and backend `test_runner`.

## Phase 3: move decoupled MIDI registration bookkeeping into Rust core

Change Rust decoupled registration API so Rust receives and stores both strong and weak bridge handle fields.

Rust should own decoupled port keepalive by retaining the bridge strong handle in `DecoupledPortRegistration`.

Rust unregister should:

- close/process via typed resolver as needed
- remove registration
- release the stored bridge strong handle

C++ decoupled port creation should remain in C++ for now, because it constructs C++ `MidiPort`/`DecoupledMidiPort` objects. Replace `AudioMidiDriverRuntime::make_decoupled_midi_port` with a small helper or direct code in concrete drivers/base class that:

- constructs the decoupled C++ object
- registers it with bridge object
- downgrades strong to weak
- queues/executes Rust core registration as needed
- sets the returned registry handle on the C++ decoupled object

Remove any C++ long-lived decoupled keepalive map. Rust must own the bridge strong handle record.

Milestone validation: run `cargo build`, `cargo test`, backend `test_runner`, and existing decoupled stress/keepalive/stale tests.

## Phase 4: remove `AudioMidiDriverRuntime`

Do not try to delete `AudioMidiDriverRuntime` in one large edit. First replace it with a shared base/helper layer that has the same remaining adapter responsibilities but no long-lived registration ownership. Then move concrete drivers to that layer and delete the old runtime files.

### Phase 4A: introduce shared C++ base/helper layer

Move the remaining non-owning adapter logic from `AudioMidiDriverRuntime` into the C++ `AudioMidiDriver` base class, or into a new small helper owned by/embedded in that base. Prefer the base class unless inheritance constraints make it awkward, because Dummy and JACK currently share almost all runtime forwarding behavior.

The shared layer may own only compatibility/adaptation state that Rust cannot represent directly:

- `rust::Box<backend_rust::AudioMidiDriverCore>` as the canonical core owner.
- `std::vector<std::weak_ptr<HasAudioProcessingFunction>>` only to preserve public `processors()` return behavior.
- client-name `std::string` cache for `const char*` return compatibility.
- maybe-process callback pointer if not moved into Rust yet.

The shared layer must not own processor bridge strong-handle maps or decoupled bridge strong-handle maps. Those records must stay in `AudioMidiDriverCore`.

Provide protected/public base methods replacing the old runtime methods, e.g.:

- `core_process(uint32_t nframes)` or `process_runtime(uint32_t nframes)`
- `runtime_add_processor`, `runtime_remove_processor`, `runtime_processors`
- `runtime_make_decoupled_midi_port`, `runtime_unregister_decoupled_midi_port`
- state forwarding: xruns, dsp load, sample rate, buffer size, client name, client handle, active, last processed
- command queue helpers: `queue_process_thread_command`, `exec_process_thread_command`, `get_command_queue`, `wait_process`

Keep any helper that constructs C++ objects small and non-owning. Rust cannot construct `DecoupledMidiPort`, so the C++ helper should still:

1. construct the C++ decoupled port,
2. register it with `bridge_object`,
3. downgrade the strong handle,
4. queue/register strong+weak scalar handle fields in `AudioMidiDriverCore`,
5. set the returned registry handle on the C++ object.

### Phase 4B: migrate concrete drivers off `m_runtime`

Update `DummyAudioMidiDriver` and `GenericJackAudioMidiDriver` to stop including `AudioMidiDriverRuntime.h` and stop declaring `AudioMidiDriverRuntime m_runtime`.

Replace calls mechanically with base/shared-layer calls. Examples:

- `m_runtime.process(nframes)` -> `core_process(nframes)` / chosen base method.
- `m_runtime.report_xrun()` -> `report_xrun()` / chosen base method.
- `m_runtime.set_sample_rate(...)` -> base/core setter.
- `m_runtime.get_maybe_client_handle()` -> base/core getter.
- `m_runtime.make_decoupled_midi_port(...)` -> base/helper method.
- `m_runtime.unregister_decoupled_midi_port(...)` -> base/helper method.
- `m_runtime.add_processor(...)` / `remove_processor(...)` / `processors()` -> base compatibility methods.
- `m_runtime.get_command_queue()` -> base/core queue accessor.

Constructor changes:

- Remove `m_runtime(maybe_process_callback)` from concrete-driver initializer lists.
- Pass `maybe_process_callback` to the `AudioMidiDriver` base constructor, or explicitly initialize the new helper/base state.
- Keep concrete Rust driver members such as `m_rust(backend_rust::new_dummy_audio_midi_driver())` unchanged.

### Phase 4C: delete old runtime files

Once no concrete driver or helper references `AudioMidiDriverRuntime`:

- delete `src/backend/internal/AudioMidiDriverRuntime.h`,
- delete `src/backend/internal/AudioMidiDriverRuntime.cpp`,
- update CMake/source lists if those files are explicitly listed,
- clean includes that referenced `AudioMidiDriverRuntime.h`,
- confirm `rg "AudioMidiDriverRuntime|m_runtime" src/backend/internal src/rust/backend_rust/src` finds no live references.

The final code should have no member `m_runtime` and no `AudioMidiDriverRuntime` references except perhaps deleted-file history.

Milestone validation: `cargo build`, backend `test_runner`.

## Phase 5: cleanup, tests, strict verification

Search and confirm:

- no `AudioMidiDriverRuntime` class references remain
- no `m_runtime` member remains in drivers
- no C++ map owns processor bridge strong handles
- no C++ map owns decoupled bridge strong handles
- Rust `AudioMidiDriverCore` owns processor and decoupled bridge strong records
- old raw/lifetime-authoritative pointer patterns are not reintroduced
- old handle-based processor/decoupled trampolines are not reintroduced

Clean comments and includes.

Run final verification:

- `cargo fmt --all`
- `RUSTFLAGS="-D warnings" cargo build`
- `cargo test`
- backend Catch2 `test_runner`
- `./target/debug/shoopdaloop_dev.sh --self-test`

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

If dependency/environment failures occur outside project code, report the logs and stop. Otherwise fix project warnings/errors introduced by the task.
