# TODO: Rust JACK API bridge refactor

## Preparation

- [ ] Inspect current C++ JACK code in `src/backend/internal/jack` and confirm all calls currently made through `IJackApi`.
- [ ] Inspect `src/rust/backend_rust/src/rust_bridge_object.rs` and one existing bridge-object CXX module to mirror the pattern.
- [ ] Add `jack-sys` with `dynamic_loading` to `src/rust/backend_rust/Cargo.toml`.

## Rust trait and implementations

- [ ] Create `src/rust/backend_rust/src/jack_api.rs`.
- [ ] Define `JackApi: Send + Sync` with the JACK operations needed by the C++ driver/ports.
- [ ] Define `JackApiObject { inner: Box<dyn JackApi> }` and delegation helpers.
- [ ] Implement `RealJackApi` using `jack-sys` and dynamic runtime JACK loading.
- [ ] Implement `TestJackApi` with instance-owned mock clients, ports, buffers, MIDI buffers, and connection state.
- [ ] Ensure test API behavior matches existing C++ `JackTestApi` defaults: sample rate 48000, buffer size 2048, CPU load 0.0, no processing support, prepopulated test clients/ports.
- [ ] Add safe/string-returning helpers in Rust for port lists and connections so C++ no longer handles JACK-owned `const char**` arrays.
- [ ] Add low-allocation MIDI event get/write/clear/count methods for process-thread use.

## CXX bridge

- [ ] Create `src/rust/backend_rust/src/jack_api_cxx.rs`.
- [ ] Define `JackApiBridgeStrong` and `JackApiBridgeWeak` with `define_rust_bridge_object_wrappers!`.
- [ ] Expose `new_jack_api()` and `new_jack_test_api()`.
- [ ] Expose bridge-object lifecycle functions such as `clone_strong`, `downgrade`, `valid`, and counts.
- [ ] Expose CXX functions for all JACK operations required by C++.
- [ ] Expose CXX test helper functions for resetting the test API and manipulating/reading mock MIDI buffers.
- [ ] Add callback registration bridge functions accepting a client handle and C++ driver pointer.
- [ ] Add the new Rust modules to `lib.rs`.
- [ ] Add the new bridge file and rerun-if-changed entries to `src/rust/backend_rust/build.rs`.

## Callback trampolines

- [ ] Add `src/backend/internal/jack/JackApiCxxTrampolines.h`.
- [ ] Add `src/backend/internal/jack/JackApiCxxTrampolines.cpp`.
- [ ] Declare C++ trampoline functions for process, xrun, port update, error logging, and info logging.
- [ ] Call these trampolines from Rust JACK callbacks registered through `jack-sys`.
- [ ] Adjust `JackAudioMidiDriver` visibility or friend declarations so trampolines can call instance callback methods safely.

## Switch C++ driver and ports to Rust bridge object API

- [ ] Replace `std::shared_ptr<IJackApi>` in `JackAudioMidiDriver` with `rust::Box<backend_rust::JackApiBridgeStrong>`.
- [ ] Make the real JACK driver constructor call `backend_rust::new_jack_api()`.
- [ ] Make the test JACK driver constructor call `backend_rust::new_jack_test_api()` or accept an injected API bridge object.
- [ ] Add a `clone_jack_api()` helper on `JackAudioMidiDriver` if needed by tests.
- [ ] Replace all driver API calls with `backend_rust::jack_api_*` bridge calls.
- [ ] Replace C++ callback registration with Rust bridge callback registration using `reinterpret_cast<uintptr_t>(this)`.
- [ ] Convert `JackAllPorts` to store a cloned `JackApiBridgeStrong` and use Rust bridge functions.
- [ ] Convert `JackPort` to store a cloned `JackApiBridgeStrong` and opaque integer client/port handles.
- [ ] Convert `JackAudioPort` buffer acquisition to use Rust bridge buffer handles.
- [ ] Convert `JackMidiInputPort` and `JackMidiOutputPort` to use Rust bridge MIDI event functions.
- [ ] Keep `maybe_driver_handle()` returning an opaque `void*` derived from the port handle for compatibility with existing public/test code.

## Update tests

- [ ] Update `src/backend/test/unit/test_JackPorts.cpp` to stop including JACK headers directly if no longer needed.
- [ ] Remove unused `jack_midi_event_t` helpers from the test file.
- [ ] Replace `JackTestApi::internal_reset_api()` with the Rust test API reset helper.
- [ ] Replace direct `JackTestApi::internal_port_data(...).midi_buffer` mutation with Rust CXX test helper calls.
- [ ] Preserve all existing JACK port test cases and assertions.
- [ ] Run `cargo build` to compile the CXX bridge and C++ code.
- [ ] Run the JACK port tests via `test_runner "[JackPorts]"` and fix failures before continuing.

## Remove obsolete C++ API code

- [ ] Delete the C++ `IJackApi` interface and real `JackApi` implementation.
- [ ] Delete the C++ `JackTestApi` implementation and header, or reduce any remaining header to a temporary compatibility include only if unavoidable.
- [ ] Remove obsolete includes of `JackApi.h`, `JackTestApi.h`, `jack_wrappers.h`, and direct JACK headers from C++ JACK driver/port code where possible.
- [ ] Delete `jack_wrappers.cpp`/`jack_wrappers.h` if no longer used.
- [ ] Update `src/backend/CMakeLists.txt` so the C++ backend no longer links to JACK for runtime calls.
- [ ] Verify with `rg "IJackApi|JackTestApi|jack_wrappers|GenericJack" src/backend src/rust/backend_rust/src` that no stale C++ implementation references remain.

## Final validation

- [ ] Run `cargo fmt --all`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build`.
- [ ] Run `cargo test` where practical.
- [ ] Run the C++ JACK tests with `test_runner "[JackPorts]"`.
- [ ] Run broader C++ tests with `test_runner` if feasible.
- [ ] Confirm the real JACK backend handles missing JACK as a runtime initialization failure, not as an application startup/linker failure.
- [ ] Confirm no old C++ JACK API implementation remains and functionality/test coverage is preserved.
