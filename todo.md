# TODO: Rust JACK API bridge refactor

## Preparation

- [x] Inspect current C++ JACK code in `src/backend/internal/jack` and confirm all calls currently made through `IJackApi`.
  - Confirmed `JackAudioMidiDriver`, `JackAllPorts`, `JackPort`, `JackAudioPort`, and `JackMidiPort` all route JACK behavior through `std::shared_ptr<IJackApi>` today.
- [x] Inspect `src/rust/backend_rust/src/rust_bridge_object.rs` and one existing bridge-object CXX module to mirror the pattern.
  - Confirmed the Rust-side `define_rust_bridge_object_wrappers!` pattern is the right basis for the JACK API bridge object.
- [x] Add `jack-sys` with `dynamic_loading` to `src/rust/backend_rust/Cargo.toml`.
  - Already present: `jack-sys = { version = "0.5", features = ["dynamic_loading"] }`.

## Rust trait and implementations

- [x] Create `src/rust/backend_rust/src/jack_api.rs`.
- [x] Define `JackApi: Send + Sync` with the JACK operations needed by the C++ driver/ports.
- [x] Define `JackApiObject { inner: Box<dyn JackApi> }` and delegation helpers.
- [x] Implement `RealJackApi` using `jack-sys` and dynamic runtime JACK loading.
- [x] Implement `TestJackApi` with instance-owned mock clients, ports, buffers, MIDI buffers, and connection state.
- [x] Ensure test API behavior matches existing C++ `JackTestApi` defaults: sample rate 48000, buffer size 2048, CPU load 0.0, no processing support, prepopulated test clients/ports.
- [x] Add safe/string-returning helpers in Rust for port lists and connections so C++ no longer handles JACK-owned `const char**` arrays.
- [x] Add low-allocation MIDI event get/write/clear/count methods for process-thread use.

## CXX bridge

- [x] Create `src/rust/backend_rust/src/jack_api_cxx.rs`.
- [x] Define `JackApiBridgeStrong` and `JackApiBridgeWeak` with `define_rust_bridge_object_wrappers!`.
- [x] Expose `new_jack_api()` and `new_jack_test_api()`.
- [x] Expose bridge-object lifecycle functions such as `clone_strong`, `downgrade`, `valid`, and counts.
- [x] Expose CXX functions for all JACK operations required by C++.
- [x] Expose CXX test helper functions for resetting the test API and manipulating/reading mock MIDI buffers.
- [x] Add callback registration bridge functions accepting a client handle and C++ driver pointer.
- [x] Add the new Rust modules to `lib.rs`.
- [x] Add the new bridge file and rerun-if-changed entries to `src/rust/backend_rust/build.rs`.

## Callback trampolines

- [x] Add `src/backend/internal/jack/JackApiCxxTrampolines.h`.
- [x] Add `src/backend/internal/jack/JackApiCxxTrampolines.cpp`.
  - The actual implementations are compiled into `backend_rust_cxx` from `src/rust/backend_rust/cxx/jack_api_trampolines.cpp` so Rust/CXX callback references resolve without static-library link-order problems.
- [x] Declare C++ trampoline functions for process, xrun, port update, error logging, and info logging.
- [x] Call these trampolines from Rust JACK callbacks registered through `jack-sys`.
- [x] Adjust `JackAudioMidiDriver` visibility or friend declarations so trampolines can call instance callback methods safely.

## Switch C++ driver and ports to Rust bridge object API

- [x] Replace `std::shared_ptr<IJackApi>` in `JackAudioMidiDriver` with `rust::Box<backend_rust::JackApiBridgeStrong>`.
- [x] Make the real JACK driver constructor call `backend_rust::new_jack_api()`.
- [x] Make the test JACK driver constructor call `backend_rust::new_jack_test_api()` or accept an injected API bridge object.
- [x] Add a `clone_jack_api()` helper on `JackAudioMidiDriver` if needed by tests.
- [x] Replace all driver API calls with `backend_rust::jack_api_*` bridge calls.
- [x] Replace C++ callback registration with Rust bridge callback registration using `reinterpret_cast<uintptr_t>(this)`.
- [x] Convert `JackAllPorts` to store a cloned `JackApiBridgeStrong` and use Rust bridge functions.
- [x] Convert `JackPort` to store a cloned `JackApiBridgeStrong` and opaque integer client/port handles.
- [x] Convert `JackAudioPort` buffer acquisition to use Rust bridge buffer handles.
- [x] Convert `JackMidiInputPort` and `JackMidiOutputPort` to use Rust bridge MIDI event functions.
- [x] Keep `maybe_driver_handle()` returning an opaque `void*` derived from the port handle for compatibility with existing public/test code.

## Update tests

- [x] Update `src/backend/test/unit/test_JackPorts.cpp` to stop including JACK headers directly if no longer needed.
- [x] Remove unused `jack_midi_event_t` helpers from the test file.
- [x] Replace `JackTestApi::internal_reset_api()` with the Rust test API reset helper.
- [x] Replace direct `JackTestApi::internal_port_data(...).midi_buffer` mutation with Rust CXX test helper calls.
- [x] Preserve all existing JACK port test cases and assertions.
- [x] Run `cargo build` to compile the CXX bridge and C++ code.
- [x] Run the JACK port tests via `test_runner "[JackPorts]"` and fix failures before continuing.

## Remove obsolete C++ API code

- [x] Delete the C++ `IJackApi` interface and real `JackApi` implementation.
- [x] Delete the C++ `JackTestApi` implementation and header, or reduce any remaining header to a temporary compatibility include only if unavoidable.
- [x] Remove obsolete includes of `JackApi.h`, `JackTestApi.h`, `jack_wrappers.h`, and direct JACK headers from C++ JACK driver/port code where possible.
- [x] Delete `jack_wrappers.cpp`/`jack_wrappers.h` if no longer used.
- [x] Update `src/backend/CMakeLists.txt` so the C++ backend no longer links to JACK for runtime calls.
  - Verified there is no runtime `libjack` dependency in `libshoopdaloop_backend.so` (`ldd ... | rg jack` returns nothing). Header discovery for JACK remains in CMake because the C++ side still includes JACK type headers.
- [x] Verify with `rg "IJackApi|JackTestApi|jack_wrappers|GenericJack" src/backend src/rust/backend_rust/src` that no stale C++ implementation references remain.

## Final validation

- [x] Run `cargo fmt --all`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build`.
- [x] Run `cargo test` where practical.
- [x] Run the C++ JACK tests with `test_runner "[JackPorts]"`.
- [x] Run broader C++ tests with `test_runner` if feasible.
- [x] Confirm the real JACK backend handles missing JACK as a runtime initialization failure, not as an application startup/linker failure.
  - Verified by implementation (`RealJackApi::init()` calls `jack_sys::library()` and returns an error) and by `ldd` showing no `libjack` dependency on the backend shared library.
- [x] Confirm no old C++ JACK API implementation remains and functionality/test coverage is preserved.
