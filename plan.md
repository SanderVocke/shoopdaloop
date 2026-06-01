# Plan: make `AudioMidiDriver` a pure abstract interface with shared Rust-backed runtime by composition

## Goal

Refactor the current C++ audio/MIDI driver hierarchy so that `AudioMidiDriver` becomes a pure abstract interface and no longer owns shared implementation state. The shared behavior that currently lives in `AudioMidiDriver` should be extracted into a composed helper object, tentatively named `AudioMidiDriverRuntime`, which owns the Rust-backed `backend_rust::AudioMidiDriverCore`, the Rust command queue, processor registration bookkeeping, and decoupled MIDI registration bookkeeping.

At the end of this task:

- `AudioMidiDriver` is an interface used by the rest of the backend through `shoop_shared_ptr<AudioMidiDriver>`.
- `AudioMidiDriver` no longer owns `backend_rust::AudioMidiDriverCore`, `backend_rust::CommandQueue`, processor containers, decoupled MIDI containers, or callback state.
- `GenericJackAudioMidiDriver<API>` and `DummyAudioMidiDriver<Time, Size>` each own an `AudioMidiDriverRuntime` member and forward shared behavior to it.
- The current public C++ and C API behavior is preserved.
- The existing Rust CXX bridges continue to provide the Rust-backed state and process-cycle implementation.
- The project builds cleanly with warnings denied, Rust code is formatted, and the relevant test suites pass.

This is an architecture refactor, not a feature change. Avoid changing external behavior except where needed to preserve correctness during the refactor.

## Current structure to understand first

Relevant C++ files:

- `src/backend/internal/AudioMidiDriver.h`
- `src/backend/internal/AudioMidiDriver.cpp`
- `src/backend/internal/AudioMidiDriverCxxTrampolines.h`
- `src/backend/internal/AudioMidiDriverCxxTrampolines.cpp`
- `src/backend/internal/DummyAudioMidiDriver.h`
- `src/backend/internal/DummyAudioMidiDriver.cpp`
- `src/backend/internal/DummyAudioMidiDriverCxxTrampolines.h`
- `src/backend/internal/jack/JackAudioMidiDriver.h`
- `src/backend/internal/jack/JackAudioMidiDriver.cpp`
- `src/backend/internal/DecoupledMidiPort.h`
- `src/backend/internal/DecoupledMidiPort.cpp`
- `src/backend/internal/AudioMidiDrivers.h`
- `src/backend/internal/AudioMidiDrivers.cpp`
- `src/backend/libshoopdaloop_backend.cpp`

Relevant Rust files:

- `src/rust/backend_rust/src/audio_midi_driver.rs`
- `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`
- `src/rust/backend_rust/src/dummy_audio_midi_driver.rs`
- `src/rust/backend_rust/src/dummy_audio_midi_driver_cxx.rs`

The current `AudioMidiDriver` C++ class is both an abstract base class and a shared implementation class. It owns:

- `rust::Box<backend_rust::AudioMidiDriverCore> m_rust_core`
- `rust::Box<backend_rust::CommandQueue> m_command_queue`
- processor weak pointers and pointer-to-handle mappings
- decoupled MIDI keepalive and registration mappings
- the optional process callback pointer

It also implements process-cycle delegation, state getters/setters, processor registration, command queue forwarding, wait behavior, and decoupled MIDI port creation/unregistration.

The Rust `AudioMidiDriverCore` already owns most low-level shared state: atomic driver state, processor handle table, decoupled port handle table, and `process_cycle()`, which calls back into C++ through `AudioMidiDriverCxxTrampolines.h` functions.

The C++ JACK and Dummy drivers currently inherit this shared behavior from `AudioMidiDriver`. This task changes that relationship to composition.

## Design constraints

Keep the external API stable. Existing users of `shoop_shared_ptr<AudioMidiDriver>` and the C API in `libshoopdaloop_backend.cpp` should not need behavior changes.

Do not duplicate the current shared implementation separately in JACK and Dummy. Extract one composed helper and have both concrete drivers use it.

Keep the Rust CXX bridge ABI as stable as practical. The Rust `AudioMidiDriverCore` and its `audio_midi_driver_cxx.rs` bridge probably do not need substantial changes for this milestone.

Preserve process-thread semantics. In particular:

- `process_cycle()` must still invoke the optional maybe-process callback before command execution.
- process-thread commands must still run through the same Rust command queue behavior.
- decoupled MIDI ports must still be processed each cycle.
- registered processors must still receive `PROC_process(nframes)`.
- `wait_process()` must retain its existing synchronization intent.

Preserve `DecoupledMidiPort` behavior. It currently stores a weak pointer to `AudioMidiDriver`; for this milestone it is acceptable for the pure interface to retain `shoop_enable_shared_from_this<AudioMidiDriver>` or a small protected accessor solely to support that relationship. Removing that relationship can be a later migration.

Minimize unrelated changes. Port classes, backend session code, and frontend bindings should only be changed if required by compile errors or API forwarding changes.

## Phase 1: extract `AudioMidiDriverRuntime` while keeping `AudioMidiDriver` behavior unchanged

The first phase should be a low-risk mechanical extraction. `AudioMidiDriver` may still own the runtime helper in this phase and forward to it. The purpose is to separate the shared implementation from the base class before changing the inheritance architecture.

Create new files under `src/backend/internal/`, for example:

- `AudioMidiDriverRuntime.h`
- `AudioMidiDriverRuntime.cpp`

CMake currently uses `file(GLOB INTERNAL_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/*.cpp)` in `src/backend/CMakeLists.txt`, so a new `.cpp` under `internal/` should be picked up automatically by the backend build. Still verify by building.

Move the implementation state from `AudioMidiDriver` into `AudioMidiDriverRuntime`:

- Rust core object: `rust::Box<backend_rust::AudioMidiDriverCore>`
- Rust command queue: `rust::Box<backend_rust::CommandQueue>`
- processor weak pointer vector
- processor pointer-to-handle map
- decoupled MIDI handle-to-shared-pointer map
- optional maybe-process callback pointer

Move or recreate these operations on `AudioMidiDriverRuntime`:

- constructor taking `void (*maybe_process_callback)()`
- `add_processor()`
- `remove_processor()`
- `processors()`
- `process(uint32_t nframes)` corresponding to the old `AudioMidiDriver::PROC_process()`
- `report_xrun()`
- `set_dsp_load()` / `get_dsp_load_raw()` or equivalent
- `set_sample_rate()` / `get_sample_rate_raw()` or equivalent
- `set_buffer_size()` / `get_buffer_size_raw()` or equivalent
- `set_maybe_client_handle()` / `get_maybe_client_handle()`
- `set_client_name()` / `get_client_name()`
- `set_active()` / `get_active()`
- `set_last_processed()` / `get_last_processed()`
- `get_xruns()` / `reset_xruns()`
- `wait_process()`
- `queue_process_thread_command()`
- `exec_process_thread_command()`
- `get_command_queue()`
- command-queue trampoline helper, if still needed during phase 1
- process trampoline helper, if still needed during phase 1
- decoupled MIDI port creation and unregistration support

For decoupled MIDI creation, avoid making the runtime depend on virtual methods. Prefer an API such as:

```cpp
shoop_shared_ptr<shoop_types::_DecoupledMidiPort> make_decoupled_midi_port(
    shoop_shared_ptr<MidiPort> port,
    shoop_weak_ptr<AudioMidiDriver> driver,
    shoop_port_direction_t direction);
```

Then `AudioMidiDriver::open_decoupled_midi_port()` can remain implemented during phase 1 as:

```cpp
auto port = open_midi_port(name, direction);
return m_runtime.make_decoupled_midi_port(port, weak_from_this(), direction);
```

Keep the existing trampoline functions declared in `AudioMidiDriverCxxTrampolines.h` available in namespace `backend_rust`:

- `audiomididriver_invoke_maybe_process_callback`
- `audiomididriver_exec_command_queue`
- `audiomididriver_process_processor`
- `audiomididriver_process_decoupled_port`
- `audiomididriver_close_decoupled_port`

They are currently implemented in `AudioMidiDriver.cpp`. Move them to `AudioMidiDriverRuntime.cpp` or to `AudioMidiDriverCxxTrampolines.cpp`, but ensure the symbols are still linked into all backend targets. The empty `AudioMidiDriverCxxTrampolines.cpp` comment should be updated if the implementation is moved there.

After this extraction, `AudioMidiDriver` should still compile with essentially the same public and protected methods as before, but those methods should forward to `m_runtime`. JACK and Dummy should need minimal changes in phase 1.

Phase 1 acceptance criteria:

- Behavior is intended to be unchanged.
- `AudioMidiDriver.cpp` is mostly forwarding logic plus any remaining base-specific glue.
- The Rust bridge compiles without API changes unless a very small bridge adjustment is clearly necessary.
- `cargo build` succeeds.
- `cargo test` succeeds.
- The backend Catch2 `test_runner` succeeds.

## Phase 2: make `AudioMidiDriver` a pure abstract interface and move the runtime into concrete drivers

After phase 1 passes, remove the runtime ownership and implementation forwarding from `AudioMidiDriver`.

Change `AudioMidiDriver.h` so the class is an interface. It should declare virtual methods for the behavior the rest of the backend uses, including at least:

- `start(AudioMidiDriverSettingsInterface &settings)`
- `open_audio_port(...)`
- `open_midi_port(...)`
- `open_decoupled_midi_port(...)`
- `unregister_decoupled_midi_port(...)`
- `close()`
- `add_processor(...)`
- `remove_processor(...)`
- `processors() const`
- `get_xruns() const`
- `get_dsp_load()`
- `get_sample_rate()`
- `get_buffer_size()`
- `reset_xruns()`
- `get_client_name() const`
- `get_maybe_client_handle() const`
- `get_active() const`
- `get_last_processed() const`
- `wait_process()`
- `queue_process_thread_command(...)`
- `exec_process_thread_command(...)`
- `get_command_queue()`
- `find_external_ports(...)`

Remove implementation-detail methods from the interface where possible. Methods like `report_xrun()`, `set_sample_rate()`, `set_buffer_size()`, `set_dsp_load()`, `set_active()`, `set_last_processed()`, `PROC_process()`, and `trampoline_process()` should become concrete-driver/runtime details rather than base-class behavior.

If `DecoupledMidiPort` still needs a weak pointer to `AudioMidiDriver`, keep `AudioMidiDriver` derived from `shoop_enable_shared_from_this<AudioMidiDriver>` and expose a small protected helper, for example:

```cpp
protected:
    shoop_weak_ptr<AudioMidiDriver> weak_driver_from_this();
```

This preserves current decoupled-port API behavior without keeping the Rust core in the base class. Use protected or private inheritance/access as appropriate, but ensure concrete drivers can create decoupled MIDI ports safely.

Add an `AudioMidiDriverRuntime m_runtime;` member to both concrete driver families:

- `GenericJackAudioMidiDriver<API>` in `src/backend/internal/jack/JackAudioMidiDriver.h`
- `DummyAudioMidiDriver<Time, Size>` in `src/backend/internal/DummyAudioMidiDriver.h`

Initialize it from each concrete constructor using the `maybe_process_callback` argument.

For JACK, replace qualified base calls with runtime calls. Examples:

- `AudioMidiDriver::PROC_process(nframes)` becomes `m_runtime.process(nframes)`.
- `AudioMidiDriver::report_xrun()` becomes `m_runtime.report_xrun()`.
- `AudioMidiDriver::set_maybe_client_handle(...)` becomes `m_runtime.set_maybe_client_handle(...)`.
- `AudioMidiDriver::set_client_name(...)` becomes `m_runtime.set_client_name(...)`.
- `AudioMidiDriver::set_sample_rate(...)` becomes `m_runtime.set_sample_rate(...)`.
- `AudioMidiDriver::set_buffer_size(...)` becomes `m_runtime.set_buffer_size(...)`.
- `AudioMidiDriver::set_dsp_load(...)` becomes `m_runtime.set_dsp_load(...)`.
- `AudioMidiDriver::wait_process()` becomes `m_runtime.wait_process()` where JACK supports processing.

Implement all `AudioMidiDriver` interface methods in `GenericJackAudioMidiDriver<API>`, forwarding shared behavior to `m_runtime` and preserving JACK-specific update hooks. For example, `get_sample_rate()` should still call the JACK API refresh logic before returning the runtime value, and `get_dsp_load()` should still refresh from JACK before returning.

For Dummy, replace qualified base calls with runtime calls. Examples:

- startup state setup should use `m_runtime.set_sample_rate()`, `m_runtime.set_buffer_size()`, `m_runtime.set_client_name()`, `m_runtime.set_dsp_load()`, `m_runtime.set_maybe_client_handle()`, and `m_runtime.set_active()`.
- calls to `AudioMidiDriver::get_sample_rate()` and `AudioMidiDriver::get_buffer_size()` should become runtime getter calls.
- `wait_process()`, processor registration, command queue access, and decoupled MIDI methods should be implemented by forwarding to `m_runtime`.

Update the Dummy Rust process-thread trampoline path so it no longer casts `owner_ptr` to `AudioMidiDriver*`. Instead, pass the address of the runtime helper to Rust:

```cpp
m_rust->start_process_thread(
    reinterpret_cast<uintptr_t>(&m_runtime),
    m_runtime.get_sample_rate(),
    m_runtime.get_buffer_size());
```

Then in `DummyAudioMidiDriver.cpp`, implement the existing functions from `DummyAudioMidiDriverCxxTrampolines.h` by casting to `AudioMidiDriverRuntime*` and calling runtime methods:

```cpp
auto *runtime = reinterpret_cast<AudioMidiDriverRuntime *>(owner_ptr);
runtime->exec_all_commands_for_process_thread();
runtime->process(nframes);
```

Use the actual runtime method names chosen in phase 1. The Rust bridge in `dummy_audio_midi_driver_cxx.rs` can keep the same extern function names if their C++ implementations now treat `owner_ptr` as a runtime pointer.

Update tests and callers that explicitly qualify base-class implementation methods. For example, in `src/backend/test/unit/test_DummyAudioMidiDriver.cpp`, code like:

```cpp
AudioMidiDriver::add_processor(...);
```

must become a virtual/concrete call such as:

```cpp
this->add_processor(...);
```

or an explicit concrete-driver call. Search for all `AudioMidiDriver::` qualified calls and update them appropriately. Implementation files should no longer use `AudioMidiDriver::` to reach shared behavior, because the base will not implement it.

Keep `AudioMidiDrivers.cpp` factory behavior intact. It should still return `shoop_shared_ptr<AudioMidiDriver>` and use `shoop_static_pointer_cast<AudioMidiDriver>` for concrete driver instances.

Keep `libshoopdaloop_backend.cpp` behavior intact. It relies on the abstract driver pointer for public API calls and also uses dynamic casts to concrete dummy/JACK types in some places. Those concrete types should still inherit `AudioMidiDriver` so those casts remain valid.

Phase 2 acceptance criteria:

- `AudioMidiDriver` no longer includes the Rust generated `audio_midi_driver_cxx.rs.h` header unless strictly needed for interface declarations.
- `AudioMidiDriver` has no Rust core member, command queue member, processor containers, decoupled MIDI containers, or maybe-process callback member.
- JACK and Dummy each own their own `AudioMidiDriverRuntime` member.
- No shared behavior is duplicated independently between JACK and Dummy.
- Dummy process-thread trampolines no longer reinterpret the owner pointer as `AudioMidiDriver*`.
- Existing external APIs and tests continue to work.

## Final cleanup expectations

After both phases, remove dead declarations and stale comments from `AudioMidiDriver.h/.cpp`, `AudioMidiDriverCxxTrampolines.cpp`, and any affected driver files.

Prefer keeping `AudioMidiDriver.cpp` either empty except for the virtual destructor/protected shared-from-this helper, or deleting most of its former implementation if no longer needed. If an empty `.cpp` remains, keep it intentional and documented.

Ensure includes are cleaned up. The abstract interface should include as little Rust/CXX bridge detail as possible. The runtime helper should include bridge and command queue details.

If touching Rust files, run Rust formatting before final verification. Even if this task is mostly C++, the final gate requires `cargo fmt --all`.

## Build and test instructions

Project-specific build and test guidance comes from `AGENTS.md`, `.agents/build.md`, and `.agents/test.md`.

From the repository root, use these during development:

- Iterative build: `cargo build`
- Rust tests: `cargo test`
- Backend C++ tests: build first, then run the Catch2 `test_runner` executable generated under `target/`. If unsure of the exact path, locate it with `find target -type f -name test_runner` and run the appropriate debug build result.
- QML/application self-test: `./target/debug/shoopdaloop_dev.sh --self-test`

Recommended phase gates:

- Before starting: run `cargo build` to establish the baseline if time permits.
- After phase 1 extraction: run `cargo build`, `cargo test`, and backend `test_runner`.
- After phase 2 interface conversion: run `cargo build`, `cargo test`, backend `test_runner`, and the QML/application self-test.

Final required gate:

- `cargo fmt --all`
- `RUSTFLAGS="-D warnings" cargo build`
- `cargo test`
- backend Catch2 `test_runner`
- `./target/debug/shoopdaloop_dev.sh --self-test`

If a failure is clearly caused by missing external dependencies or development environment setup outside the project, report the environment blocker with the relevant logs and stop. Otherwise, fix project warnings/errors until the final gate passes.
