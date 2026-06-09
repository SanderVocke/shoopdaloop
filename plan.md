# Plan: port JACK API implementations to Rust and call them from C++ through CXX

## Goal

Move the real JACK API implementation and the JACK test/mock API implementation out of C++ and into `src/rust/backend_rust`. C++ should no longer define or own `IJackApi`, `JackApi`, or `JackTestApi`. Instead, C++ `JackAudioMidiDriver`, `JackAllPorts`, `JackPort`, `JackAudioPort`, and `JackMidiPort` should hold a Rust bridge-object strong handle to a Rust object that wraps a `Box<dyn JackApi>`, and all JACK operations should be performed through CXX-exposed Rust functions.

The real Rust JACK implementation must use runtime dynamic loading of JACK through `jack-sys` with its `dynamic_loading` feature so the application binary does not hard-link to JACK and does not fail to start solely because the JACK shared library is absent. Missing JACK should become a runtime backend-initialization error when the real JACK backend is used.

The C++ test suite must keep equivalent test coverage. C++ tests that currently access `JackTestApi::internal_port_data(...)` should use Rust/CXX test helper functions operating on the Rust test API object and opaque port handles.

## Relevant code locations

C++ JACK code is currently in:

- `src/backend/internal/jack/JackApi.h`
- `src/backend/internal/jack/JackTestApi.h`
- `src/backend/internal/jack/JackTestApi.cpp`
- `src/backend/internal/jack/JackAudioMidiDriver.h`
- `src/backend/internal/jack/JackAudioMidiDriver.cpp`
- `src/backend/internal/jack/JackAllPorts.h`
- `src/backend/internal/jack/JackAllPorts.cpp`
- `src/backend/internal/jack/JackPort.h`
- `src/backend/internal/jack/JackPort.cpp`
- `src/backend/internal/jack/JackAudioPort.h`
- `src/backend/internal/jack/JackAudioPort.cpp`
- `src/backend/internal/jack/JackMidiPort.h`
- `src/backend/internal/jack/JackMidiPort.cpp`

Driver factory / public backend API code that references JACK drivers is in:

- `src/backend/internal/AudioMidiDrivers.cpp`
- `src/backend/libshoopdaloop_backend.cpp`

The C++ JACK port tests are in:

- `src/backend/test/unit/test_JackPorts.cpp`

Rust backend crate code lives in:

- `src/rust/backend_rust/src`
- `src/rust/backend_rust/src/lib.rs`
- `src/rust/backend_rust/build.rs`
- `src/rust/backend_rust/Cargo.toml`

Rust bridge-object helpers are in:

- `src/rust/backend_rust/src/rust_bridge_object.rs`

Use those helpers for the new JACK API bridge object rather than inventing another ownership scheme.

## Architectural target

CXX cannot expose a Rust trait object directly to C++. The Rust-side shape should therefore be:

```rust
pub trait JackApi: Send + Sync {
    // JACK-like operations used by the driver and ports.
}

pub struct JackApiObject {
    inner: Box<dyn JackApi>,
}
```

Then define bridge wrappers with the existing macro:

```rust
crate::define_rust_bridge_object_wrappers!(
    JackApiBridgeStrong,
    JackApiBridgeWeak,
    JackApiObject
);
```

C++ classes should hold `rust::Box<backend_rust::JackApiBridgeStrong>` or cloned strong handles derived from one root API object. For example:

```cpp
rust::Box<backend_rust::JackApiBridgeStrong> m_api;
```

When constructing child objects such as `JackAllPorts` or ports, clone the strong handle and move the clone into the child:

```cpp
auto api_for_port = m_api->clone_strong();
```

This preserves shared ownership while matching the bridge-object migration pattern used elsewhere in the codebase.

## Rust API design

Add new Rust modules:

- `src/rust/backend_rust/src/jack_api.rs`
- `src/rust/backend_rust/src/jack_api_cxx.rs`

Register them in `lib.rs` and add the CXX bridge source to `build.rs`.

Add `jack-sys` to `src/rust/backend_rust/Cargo.toml` with dynamic loading enabled. Use approximately this shape, adjusting the version to the current compatible crate version:

```toml
jack-sys = { version = "...", features = ["dynamic_loading"] }
```

Prefer `jack-sys` over the high-level `jack` crate. The C++ side currently works with raw JACK-style client, port and buffer handles, and `jack-sys` maps much more directly to that style than the high-level ownership/lifetime model of `jack`.

The Rust trait should expose only the operations the C++ driver/ports need, not the full JACK API. Use `usize` for opaque handles passed across CXX:

- client handle: `usize`, representing `jack_client_t*` for real JACK
- port handle: `usize`, representing `jack_port_t*` for real JACK
- buffer handle: `usize`, representing raw JACK buffer pointers for real JACK

Use richer CXX-friendly return values where possible instead of mirroring JACK allocation patterns. In particular, do not expose `const char**` plus `jack_free` to C++. Prefer `Vec<String>` and `String`.

Suggested Rust/CXX-facing functions:

- `new_jack_api() -> Box<JackApiBridgeStrong>` creates the real dynamic-loading JACK API.
- `new_jack_test_api() -> Box<JackApiBridgeStrong>` creates an independent test API instance.
- bridge-object methods: `valid`, `clone_strong`, `downgrade`, `strong_count`, `weak_count` as with other bridge objects.
- `jack_api_supports_processing(api) -> bool`
- `jack_api_init(api) -> Result<()>`
- `jack_api_client_open(api, client_name: &str) -> Result<usize>`; return `0` only for an actual null client if choosing not to use `Result`, but `Result` is preferable because CXX already uses Rust `Result` in this crate.
- `jack_api_activate(api, client: usize) -> i32`
- `jack_api_deactivate(api, client: usize) -> i32`
- `jack_api_client_close(api, client: usize) -> i32`
- `jack_api_get_client_name(api, client: usize) -> String`
- `jack_api_get_sample_rate(api, client: usize) -> u32`
- `jack_api_get_buffer_size(api, client: usize) -> u32`
- `jack_api_cpu_load(api, client: usize) -> f32`
- `jack_api_port_register(api, client: usize, name: &str, is_audio: bool, is_input: bool) -> usize`
- `jack_api_port_unregister(api, client: usize, port: usize) -> i32`
- `jack_api_port_name(api, port: usize) -> String`
- `jack_api_port_get_buffer(api, port: usize, nframes: u32) -> usize`
- `jack_api_port_is_input(api, port: usize) -> bool`
- `jack_api_port_is_output(api, port: usize) -> bool`
- `jack_api_port_is_audio(api, port: usize) -> bool`
- `jack_api_port_is_midi(api, port: usize) -> bool`
- `jack_api_port_is_mine(api, client: usize, port: usize) -> bool`
- `jack_api_get_ports(api, client: usize, name_pattern: &str, use_name_pattern: bool, want_audio: bool, use_type_filter: bool, want_input: bool, use_direction_filter: bool) -> Vec<String>`
- `jack_api_port_by_name(api, client: usize, name: &str) -> usize`
- `jack_api_port_get_all_connections(api, client: usize, port: usize) -> Vec<String>`
- `jack_api_connect(api, client: usize, src: &str, dst: &str) -> i32`
- `jack_api_disconnect(api, client: usize, src: &str, dst: &str) -> i32`
- `jack_api_midi_get_event_count(api, buffer: usize) -> u32`
- `unsafe jack_api_midi_event_get(api, buffer: usize, index: u32, out_time: &mut u32, out_size: &mut u16, out_data: *mut u8) -> bool`
- `unsafe jack_api_midi_event_write(api, buffer: usize, time: u32, size: u16, data: *const u8) -> i32`
- `jack_api_midi_clear_buffer(api, buffer: usize)`

This list can be trimmed if a function is truly unused, but avoid large semantic leaps in the first implementation. The goal is to replace the current C++ API usage with minimal behavior changes.

## Callback design

Do not pass C++ callback function pointers through Rust. Instead, expose CXX functions for registering callbacks by client handle and C++ driver pointer:

- `jack_api_set_process_callback(api, client: usize, driver_ptr: usize) -> i32`
- `jack_api_set_xrun_callback(api, client: usize, driver_ptr: usize) -> i32`
- `jack_api_set_port_connect_callback(api, client: usize, driver_ptr: usize) -> i32`
- `jack_api_set_port_registration_callback(api, client: usize, driver_ptr: usize) -> i32`
- `jack_api_set_port_rename_callback(api, client: usize, driver_ptr: usize) -> i32`
- `jack_api_set_error_info_logging(api)` or equivalent initialization for JACK error/info logging

For the real JACK implementation, these functions should register Rust `extern "C"` callbacks with `jack-sys`. The Rust callback receives the `driver_ptr` as its callback argument and calls C++ trampolines declared through CXX.

Add a C++ trampoline header and implementation, for example:

- `src/backend/internal/jack/JackApiCxxTrampolines.h`
- `src/backend/internal/jack/JackApiCxxTrampolines.cpp`

The header is included from `jack_api_cxx.rs` as an unsafe C++ include. It should declare functions in namespace `backend_rust`, for example:

```cpp
namespace backend_rust {
int jackapi_process_cb(uintptr_t driver_ptr, uint32_t nframes);
int jackapi_xrun_cb(uintptr_t driver_ptr);
void jackapi_port_update_cb(uintptr_t driver_ptr);
void jackapi_error_log(rust::Str msg);
void jackapi_info_log(rust::Str msg);
}
```

The implementations should cast `driver_ptr` back to `JackAudioMidiDriver*` and call instance methods such as `PROC_process_cb_inst`, `PROC_xrun_cb_inst`, and `PROC_update_ports_cb_inst`. Make those instance methods public or declare the trampoline as a friend; do not use template-specific static callbacks anymore. Logging trampolines should preserve the current logging behavior.

For the Rust test API, callback registration can store enough callback state to emulate the current behavior. At minimum, port registration should trigger the port-update trampoline when new ports are registered, matching the current C++ test API behavior. Processing support should remain false for the test API unless deliberately extended.

## Real Rust JACK implementation

Implement a `RealJackApi` struct in Rust. It can be mostly stateless. It should call `jack_sys` functions inside small unsafe blocks and convert between raw pointers and `usize` at the bridge boundary.

Important requirements:

- Use `jack-sys` with `dynamic_loading` so real JACK is not a hard runtime dependency.
- `init` should attempt whatever dynamic initialization/loading `jack-sys` requires and return a clear error if the JACK library cannot be loaded.
- `client_open` should convert the Rust `&str` client name into a `CString`, call JACK, and return the client pointer as `usize` or an error/null result.
- Methods returning strings should handle null pointers and invalid UTF-8 defensively. Return an empty string on null if the C++ caller can handle that, or return an error for functions where null is exceptional.
- Methods returning port lists or connection lists should copy C strings into Rust `String`s and free JACK-owned arrays on the Rust side before returning `Vec<String>` to C++.
- MIDI event get/write should copy at most four bytes because the existing `MidiStorageElem` is a 4-byte inline MIDI representation.
- JACK error/info callbacks are global in JACK. Register Rust callbacks that forward messages through the C++ logging trampolines.

## Rust test JACK implementation

Implement a `TestJackApi` struct in Rust, also implementing the `JackApi` trait. It should be instance-owned, not a global static singleton, so tests can create independent APIs. Internally use a `Mutex` around test state.

Preserve current C++ test API behavior:

- `init` populates two mock clients, `test_client_1` and `test_client_2`, each with `audio_in`, `audio_out`, `midi_in`, and `midi_out` ports.
- `client_open("test", ...)` creates or returns a client named `test`.
- `port_register` opens a client-local port and triggers the registered port-registration/update callback.
- `get_ports` returns all matching ports across test clients, with names in `client:port` format.
- `port_by_name` supports both `client:port` names and client-local names.
- `connect` and `disconnect` update connection sets on both ports.
- `port_get_buffer` returns an audio buffer pointer for audio ports and a MIDI buffer handle for MIDI ports.
- MIDI event count/get/write/clear operate on the mock port's MIDI buffer.
- `supports_processing` returns false.
- sample rate is `48000`, buffer size is `2048`, CPU load is `0.0`, matching the current C++ test API.

Use stable handles for test clients and ports. A good approach is to store clients and ports as `Box<TestClient>` / `Box<TestPort>` inside the test state and use their stable addresses as opaque `usize` handles. When looking up a handle, prefer looking it up by comparing stored boxed addresses inside the locked state rather than blindly dereferencing raw pointers. For buffers, a MIDI buffer handle can simply be the mock port handle. Audio buffer pointers can be returned from the port's `Vec<f32>` after resizing; this is sufficient for the existing single-threaded tests.

Expose CXX test helper functions operating on `JackApiBridgeStrong`. They should return false or throw a clear error when the API object is not a `TestJackApi`. Suggested helpers:

- `jack_test_api_reset(api)` resets the test API state.
- `unsafe jack_test_api_push_midi_event(api, port: usize, time: u32, size: u16, data: *const u8) -> bool`
- `jack_test_api_clear_midi_buffer(api, port: usize) -> bool`
- `jack_test_api_midi_buffer_size(api, port: usize) -> usize`
- `unsafe jack_test_api_get_midi_event(api, port: usize, index: usize, out_time: &mut u32, out_size: &mut u16, out_data: *mut u8) -> bool`

These helpers replace direct C++ access to `JackTestApi::internal_port_data(...).midi_buffer`.

## CXX bridge integration

Add `src/rust/backend_rust/src/jack_api_cxx.rs` with a `#[cxx::bridge(namespace = "backend_rust")]` module. It should expose:

- `JackApiBridgeStrong` and `JackApiBridgeWeak`
- constructor functions for real and test APIs
- bridge-object lifecycle methods (`valid`, `clone_strong`, `downgrade`, counts)
- JACK API functions listed above
- test helper functions
- unsafe C++ trampolines for callbacks/logging

Add the bridge source to `cxx_build::bridges([...])` in `src/rust/backend_rust/build.rs`. Add `cargo:rerun-if-changed` lines for the new Rust bridge file and the C++ trampoline header.

Include the generated header from C++ where needed:

```cpp
#include "backend_rust/src/jack_api_cxx.rs.h"
```

## C++ JACK driver and port changes

Replace `std::shared_ptr<IJackApi>` with Rust bridge-object strong handles throughout the C++ JACK classes.

In `JackAudioMidiDriver`:

- Store `rust::Box<backend_rust::JackApiBridgeStrong> m_api`.
- The default real JACK constructor should call `backend_rust::new_jack_api()`.
- The test driver constructor should call `backend_rust::new_jack_test_api()` or accept an injected test API strong handle.
- Provide a small public helper such as `clone_jack_api()` returning `m_api->clone_strong()` so C++ tests can call Rust test helper functions on the same API instance used by the driver.
- Replace every `m_api->...` C++ virtual call with `backend_rust::jack_api_...(*m_api, ...)`.
- Replace callback registration with the Rust callback-registration bridge functions that receive `reinterpret_cast<uintptr_t>(this)`.
- Store client handles as `uintptr_t`/`usize` values internally, while continuing to use `set_maybe_client_handle(reinterpret_cast<void*>(client_handle))` if the public `AudioMidiDriver` state still stores the raw handle as `void*`.
- `wait_process()` should call `jack_api_supports_processing`.

In `JackAllPorts`:

- Store a cloned `JackApiBridgeStrong`.
- Use `jack_api_get_ports`, `jack_api_port_by_name`, `jack_api_port_is_input`, `jack_api_port_is_audio`, and `jack_api_port_get_all_connections` instead of JACK C API pointer arrays and flags.

In `JackPort`:

- Store a cloned `JackApiBridgeStrong`.
- Store `uintptr_t m_client` and `uintptr_t m_port` instead of `jack_client_t*` and `jack_port_t*` if possible.
- Register ports using `jack_api_port_register` with `is_audio` and `is_input` booleans.
- Close ports using `jack_api_port_unregister`.
- Prepare buffers using `jack_api_port_get_buffer`.
- External connect/disconnect and status should use Rust bridge functions and `Vec<String>` results.
- `maybe_driver_handle()` can continue to return `reinterpret_cast<void*>(m_port)`; it is an opaque handle for C++ callers/tests.

In `JackAudioPort`:

- Treat the buffer handle returned by the Rust bridge as a raw float pointer only after casting from `uintptr_t`/`usize`.
- Keep the current fallback-buffer behavior for missing JACK buffers.

In `JackMidiPort`:

- Use `jack_api_midi_get_event_count`, `jack_api_midi_event_get`, `jack_api_midi_clear_buffer`, and `jack_api_midi_event_write`.
- Continue copying to/from `MidiStorageElem` on the C++ side.

Remove C++ dependencies on JACK headers from these classes where practical. If a header is still needed only for typedefs/constants, prefer replacing it with `cstdint` and bridge helper functions/constants. The end state should not require C++ to call or link against JACK.

## C++ test updates

Update `src/backend/test/unit/test_JackPorts.cpp`:

- Remove `#include <jack/midiport.h>` if no longer needed.
- Remove the unused `to_msg(jack_midi_event_t&)` helper if it remains unused.
- Replace `JackTestApi::internal_reset_api()` with a Rust bridge helper. If the default `JackTestAudioMidiDriver` creates its own API, use `driver->clone_jack_api()` after construction and call `backend_rust::jack_test_api_reset(*api)` before `start`, or construct the API in the test helper and inject a clone into the driver.
- Replace every `JackTestApi::internal_port_data((jack_port_t*)port->maybe_driver_handle())` use with helper functions such as `jack_test_api_push_midi_event`, `jack_test_api_clear_midi_buffer`, `jack_test_api_midi_buffer_size`, and `jack_test_api_get_midi_event`.
- Preserve all existing assertions and test cases where possible.

One workable test helper shape is:

```cpp
struct OpenedJackTestDriver {
    rust::Box<backend_rust::JackApiBridgeStrong> api;
    std::unique_ptr<JackAudioMidiDriver> driver;
};
```

However, if that disrupts many tests, instead add `clone_jack_api()` to `JackAudioMidiDriver` and keep `open_test_driver()` returning `std::unique_ptr<JackTestAudioMidiDriver>`.

## Deleting obsolete C++ JACK API code

After the C++ driver and tests are using the Rust bridge exclusively, delete or reduce the old C++ API files:

- Delete `src/backend/internal/jack/JackTestApi.cpp`.
- Delete `src/backend/internal/jack/JackTestApi.h`, or leave only a temporary compatibility include if absolutely necessary during the transition. The final code should not expose a C++ `JackTestApi` class.
- Delete C++ `IJackApi` and `JackApi` class definitions from `JackApi.h`. Ideally delete `JackApi.h` entirely and update includes to use the generated Rust bridge header instead.
- Remove use of `jack_wrappers.cpp`/`jack_wrappers.h` if nothing else uses them. If they become unused, delete them and remove any CMake references.
- Update `src/backend/CMakeLists.txt`: stop linking the backend to the JACK library for runtime calls. The C++ backend should not require `PkgConfig::jack` just to link. If JACK headers are no longer included from C++, remove the C++ JACK header dependency too. Keep the JACK backend build enabled based on the Rust dynamic-loading implementation rather than C++ link availability.

Run `rg "IJackApi|JackTestApi|jack_wrappers|jack_" src/backend/internal/jack src/backend/test/unit/test_JackPorts.cpp` and remove stale references. Some generated/real JACK naming may remain in Rust and bridge names; the goal is specifically no old C++ API implementation.

## Error handling and safety

Use `Result` for Rust bridge calls where failure is exceptional and should become a C++ exception or clear failure. Existing CXX code in this crate already exposes Rust `Result`, so follow that pattern.

Keep unsafe operations small and documented:

- converting `usize` to `jack_client_t*`, `jack_port_t*`, and buffer pointers in `RealJackApi`
- copying MIDI data through raw pointers in bridge functions
- invoking C++ callback trampolines from Rust JACK callbacks

For realtime paths, avoid allocations in per-cycle MIDI functions. Returning `Vec<String>` is fine for discovery/status functions but not for process-thread MIDI event operations.

## Build and test instructions

Project guidance from `AGENTS.md` and `.agents/build.md` says the complete project build is managed by Cargo. For final validation, run:

```bash
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

C++ tests are built as a side effect of building the backend crate. To run the JACK port tests after a successful build, locate `test_runner` and run the JACK filter, for example:

```bash
find target/debug -name test_runner -type f
/path/to/test_runner "[JackPorts]"
```

Also run Rust tests where practical:

```bash
cargo test
```

At intermediate milestones, `cargo build` is preferred because it exercises the CXX bridge generation and C++ compilation. If a standalone `cargo check --manifest-path src/rust/backend_rust/Cargo.toml` fails because the C++ include environment is incomplete, validate through the top-level `cargo build` instead, which sets up the CMake/Corrosion include paths.

The final state should have:

- no Rust warnings under `RUSTFLAGS="-D warnings"`
- all JACK C++ tests passing
- the old C++ JACK API implementation removed
- no hard runtime link from the C++ backend to JACK
- Rust code formatted with `cargo fmt --all`
