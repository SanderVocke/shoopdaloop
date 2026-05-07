# TODO — Double-Bridge for Carla LV2 Processing Chain

## M1 — Crate skeleton exists and compiles (no bridges yet)

- [ ] **1.1** Create `src/rust/carla_lv2/` directory.
- [ ] **1.2** Write `Cargo.toml` — crate `carla_lv2`, `links = "carla_lv2"`,
  crate-type `["rlib", "staticlib"]`, depends on `cxx`, build-depends on
  `cxx-build`, workspace dependencies.
- [ ] **1.3** Write a skeletal `build.rs` that gates on `prebuild` feature
  and (for now) produces a no-op main function; emit the `cargo:include`,
  `cargo:cxx_bridge_libdir` and `cargo:rust_libdir` directives with dummy
  values (or comment them out until the bridge files exist).
- [ ] **1.4** Write a skeletal `src/lib.rs` — just `pub mod inner_bridge;`
  and `pub mod outer_bridge;` commented out.
- [ ] **1.5** Add `"src/rust/carla_lv2"` to the workspace `members` array
  in root `Cargo.toml`.
- [ ] **1.6** Run `cargo check -p carla_lv2` — must succeed (no bridge
  code yet, just the crate structure).
- [ ] **1.7** Run `cargo check --workspace` — confirm the new crate does
  not break any existing crate.

**✓ Milestone check:** `cargo check --workspace` passes with the new
  crate present but inactive.

---

## M2 — C++ facade compiles in isolation (no cxx involved)

- [ ] **2.1** Write `src/backend/internal/lv2/CarlaLv2ChainCppFacade.h`.
  Declaration only; match the plan exactly (methods, no overloads, no
  templates).  Mark the class `final` so the compiler can flag accidental
  overloads.
- [ ] **2.2** Write `src/backend/internal/lv2/CarlaLv2ChainCppFacade.cpp`.
  All methods flesh out.  `create()` calls the *existing* `LV2::create_carla_chain`
  and stores the returned `shoop_shared_ptr<CarlaLV2ProcessingChain>`.
  Port-pointer methods call `m_chain->input_audio_ports()[idx].get()` and
  return `reinterpret_cast<uintptr_t>(...)`.
- [ ] **2.3** Add `CarlaLv2ChainCppFacade.cpp` to the `INTERNAL_SOURCES`
  glob in `src/backend/CMakeLists.txt` (or list it explicitly).
- [ ] **2.4** Write a small C++ test (or a temporary `main()` in the file
  guarded by `#ifdef STANDALONE_TEST`) that calls `create()`, then calls
  `n_audio_input_ports()`, `process(64)`, `serialize_state(5000)` and
  verifies they don't crash or throw.  Link against the existing backend
  libs.
- [ ] **2.5** Build & run the test — it must pass without lv2/lilv symbols
  being involved yet (or with a dummy LilvWorld if the chain constructor
  demands it; if so, skip the test and just verify the .cpp compiles).

**✓ Milestone check:** `CarlaLv2ChainCppFacade.cpp` compiles cleanly as
  part of the backend-internals library.  A hand-written test (if feasible
  without a running LV2 stack) exercises the factory and a few methods.

---

## M3 — Inner bridge compiles (Rust ↔ C++ glue generated)

- [ ] **3.1** Write `src/rust/carla_lv2/src/inner_bridge.rs` — the
  `#[cxx::bridge(namespace="carla_lv2_inner")]` module with `extern "C++"`
  declarations matching the facade's public methods exactly (signatures,
  `self` receiver style, return types).  `include!("CarlaLv2ChainCppFacade.h")`.
- [ ] **3.2** In `build.rs`, call `cxx_build::bridge("src/inner_bridge.rs")`
  (or a list-of-1) **instead of** `cxx_build::bridges` for now — we will
  add the outer bridge later.  Chain `.std("c++20")` and required
  `.include()` calls:
  - `../../backend/internal/lv2`        (the facade header)
  - `../../backend/internal`             (transitive – ProcessingChainInterface, MidiPort, …)
  - `../../backend`                      (types.h, shoop_globals.h)
  - system lilv / lv2 include dirs if cxx-build does not find them on its own.
- [ ] **3.3** Temporarily put a `#![allow(unused)]` on `src/lib.rs` and
  run `cargo build -p carla_lv2`.  Watch for:
  - “extern C++ function not found” → the facade method signature does
    not match the cxx declaration (fix).
  - compilation errors in the auto-generated `.rs.h` → method names /
    parameter types wrong (fix).
  - linking errors about missing `carla_lv2_cxx` symbols → expected at
    this stage — the `carla_lv2_cxx` static library is produced but not
    yet linked into anything with `main()`.
- [ ] **3.4** Once `cargo build -p carla_lv2` succeeds, verify that the
  generated header `target/…/cxxbridge/include/carla_lv2/src/inner_bridge.rs.h`
  contains the expected C++ function stubs (the ones cxx will call into
  the real facade).

**✓ Milestone check:** `cargo build -p carla_lv2` succeeds.  The generated
  C++ glue compiles without errors for the inner bridge alone.

---

## M4 — Outer bridge + FxChain compiles (full Rust side stands)

- [ ] **4.1** Write `src/rust/carla_lv2/src/fx_chain.rs` — the `FxChain`
  struct holding a `cxx::UniquePtr<inner::CarlaLv2ChainCppFacade>`, plus
  `new()` factory that calls `inner::create(…)`, and delegate methods for
  every outer-bridge function listed in the plan.
- [ ] **4.2** Write `src/rust/carla_lv2/src/outer_bridge.rs` — the
  `#[cxx::bridge(namespace="carla_lv2")]` module with `extern "Rust"`
  declarations that mirror the outer API.  Mark the `FxChain` type as
  opaque.
- [ ] **4.3** Update `build.rs` to use `cxx_build::bridges(&["src/inner_bridge.rs",
  "src/outer_bridge.rs"])` so both bridges are compiled into a single
  `carla_lv2_cxx` library.  Same `.include()` and `.std()` calls.
- [ ] **4.4** Update `src/lib.rs` to `pub use` the inner-bridge and
  outer-bridge FFI types, and re-export `FxChain`.
- [ ] **4.5** Run `cargo build -p carla_lv2` and fix any mismatch between
  the outer bridge signatures and the `impl FxChain` methods (cxx requires
  exact name + parameter count + return type match).
- [ ] **4.6** Write a small `#[cfg(test)]` Rust unit test in `fx_chain.rs`
  (or a `tests/` folder) that just calls `FxChain::new(…)` to ensure the
  factory path reaches the C++ constructor.  If a live LV2 stack is not
  available in the test environment, mark it `#[ignore]` for now or use
  a mock `CarlaLv2ChainCppFacade`.

**✓ Milestone check:** `cargo build -p carla_lv2` succeeds with both
  bridges.  The generated C++ header
  `include/carla_lv2/src/outer_bridge.rs.h` contains the `carla_lv2::`
  namespace with `create_fx_chain`, `process`, etc.

---

## M5 — C++ adapter compiles in isolation

- [ ] **5.1** Write `src/backend/internal/lv2/RustFxChainAdapter.h` — the
  class declared as shown in the plan, inheriting from all three interfaces.
- [ ] **5.2** Write `src/backend/internal/lv2/RustFxChainAdapter.cpp` —
  constructor initialises `m_rust_chain` from the moved-in `rust::Box`,
  then iterates port-count plus port-pointer methods to fill the cached
  shared_ptr vectors.  Every other method is a one-liner forwarding to
  `m_rust_chain->method(…)`.
- [ ] **5.3** Add `RustFxChainAdapter.cpp` to `INTERNAL_SOURCES` in CMake.
- [ ] **5.4** For the outer-bridge generated header to be found by this
  `.cpp`, ensure the `include` paths passed via `CARLA_LV2_CXX_INCLUDE`
  are on the CMake target's include path.  Temporarily hard-code a valid
  path (e.g. `target/debug/build/…/cxxbridge/include`) in `CMakeLists.txt`
  so the adapter can `#include "carla_lv2/src/outer_bridge.rs.h"`.
- [ ] **5.5** Build.  Resolve any “no such file” for the generated header
  or “incomplete type” errors from cxx-generated glue.

**✓ Milestone check:** `RustFxChainAdapter.cpp` compiles as part of the
  backend-internals library.  (Linking may still fail because the adapter
  symbols reference `carla_lv2_cxx` symbols that are not yet linked.)

---

## M6 — End-to-end build: backend + carla_lv2 linked together

- [ ] **6.1** In `src/rust/backend/build.rs`, read the three new env vars:
  `DEP_CARLA_LV2_INCLUDE`, `DEP_CARLA_LV2_CXX_BRIDGE_LIBDIR`,
  `CARGO_STATICLIB_FILE_CARLA_LV2`.  Pass them to the CMake `Config`:
  `.define("CARLA_LV2_CXX_INCLUDE", …)`, etc.
- [ ] **6.2** In `src/backend/CMakeLists.txt`, after the refilling_pool
  section, add:
  ```cmake
  include_directories(${CARLA_LV2_CXX_INCLUDE})
  link_directories(${CARLA_LV2_CXX_LIBDIR})
  ```
  And add to the `target_link_libraries(shoopdaloop_backend_internals …)`
  call: `${CARLA_LV2_RUST_LIB}` and `carla_lv2_cxx`.
- [ ] **6.3** In `src/backend/CMakeLists.txt`, also add the carla_lv2
  include directory to the **public** include directories (not just
  private) so that the generated `outer_bridge.rs.h` is visible from the
  main backend C API (`libshoopdaloop_backend.cpp`).
- [ ] **6.4** In `BackendSession.cpp`, `#include "carla_lv2/src/outer_bridge.rs.h"`
  and `#include "RustFxChainAdapter.h"`.
- [ ] **6.5** For now, keep the existing `create_fx_chain` code path
  unchanged — this step only verifies that the bridge crate compiles and
  links into the backend shared library with no undefined symbols.
- [ ] **6.6** Full workspace build: `cargo build --workspace`.  Resolve
  any duplicate-symbol, missing-symbol, or link-order errors.

**✓ Milestone check:** `cargo build --workspace` links the full
  `libshoopdaloop_backend.so/.dll` with `carla_lv2_cxx` and
  `RustFxChainAdapter` code present.  The binary simply doesn't call any
  of the new code paths yet.

---

## M7 — Wire BackendSession to use the double-bridge for Carla chains

- [ ] **7.1** In `BackendSession.cpp`, inside the `case Carla_Rack: … case
  Carla_Patchbay_16x:` block, replace the `LV2 lv2; lv2.create_carla_chain<…>`
  call with the new adapter construction:
  ```cpp
  rust::Box<carla_lv2::FxChain> rust_chain = carla_lv2::create_fx_chain(
      static_cast<uint32_t>(type), m_sample_rate, m_buffer_size,
      std::string(title));
  chain = shoop_static_pointer_cast<ProcessingChainInterface<Time, Size>>(
      shoop_make_shared<RustFxChainAdapter>(std::move(rust_chain)));
  ```
- [ ] **7.2** The code uses `Time` and `Size` aliases which should be
  compatible with `RustFxChainAdapter`'s definitions.  Verify the types
  align (both are `uint32_t` / `uint16_t`).  If `RustFxChainAdapter` is
  hard-coded for `<uint32_t, uint16_t>`, this is fine.
- [ ] **7.3** Build.  Fix any include issues, type mismatches, or missing
  `#include`s.
- [ ] **7.4** Check that the old `LV2` singleton is no longer needed for
  the Carla case.  (It is still used for lilv world initialisation inside
  the facade's `create()`, so the `LV2` class itself stays in the codebase
  — it is just no longer called from `BackendSession`.)

**✓ Milestone check:** `cargo build --workspace` succeeds with
  `BackendSession::create_fx_chain` routed through the double-bridge for
  all three Carla chain types.

---

## M8 — Smoke-test the double-bridge at runtime

- [ ] **8.1** Build the full application (Rust + C++ + Qt/QML if available
  in your environment).
- [ ] **8.2** Launch the application (or the backend tests) with a clean
  profile / empty config so no persisted state interferes.
- [ ] **8.3** Manually trigger creation of a Carla Rack FX chain:
  - If a GUI is available: add a Carla Rack in the UI.
  - If using the test harness: call `create_fx_chain(Carla_Rack, "test")`.
- [ ] **8.4** Verify the chain enters “ready” state (logs from
  `CarlaLV2ProcessingChain` will appear, indicating the C++ chain was
  instantiated behind the bridges).
- [ ] **8.5** Verify basic audio processing works: connect an audio loop
  output to the Carla rack input, play audio, verify it (optionally)
  passes through or the rack shows up in process profiling.
- [ ] **8.6** Check port counts — `n_fx_chain_audio_input_ports` via the
  existing C API should return the correct number (2 for Rack/Patchbay,
  16 for Patchbay 16x).
- [ ] **8.7** If the application crashes: check stack traces for null
  pointer dereference in the adapter's port-caching constructor.  Add
  null checks and logging.

**✓ Milestone check:** Application starts, a Carla chain can be created,
  ports are visible, no crash during creation.

---

## M9 — UI show/hide through the double-bridge

- [ ] **9.1** Trigger `show()` on the Carla chain (via UI or C API
  `fx_chain_set_ui_visible(chain, 1)`).  Verify the Carla UI window
  appears.
- [ ] **9.2** Trigger `hide()` — window closes, UI thread joins cleanly
  (no zombie threads or leaked handles).
- [ ] **9.3** Toggle show/hide multiple times, watching for:
  - “UI thread already joined” / double-join errors.
  - “busy making visible” races.
  - Mutex deadlocks (the existing code uses `std::recursive_mutex` for
    UI thread synchronisation; the double-bridge path should preserve
    this).
- [ ] **9.4** Verify `visible()` returns the correct boolean after show
  and hide.

**✓ Milestone check:** UI window can be shown and hidden repeatedly
  without crash or thread leak.

---

## M10 — State serialise / deserialise through the double-bridge

- [ ] **10.1** Create a Carla Rack, add some internal plugins via its UI
  (e.g. a simple filter).
- [ ] **10.2** Call `serialize_state(5000)` (via C API
  `get_fx_chain_internal_state`).  Check that the returned string is a
  valid JSON object with base64 entries.
- [ ] **10.3** Call `deserialize_state(str)` with the same string on a
  *different* Carla rack instance and verify the internal plugins
  reappear.
- [ ] **10.4** Test the timeout path: create a chain but somehow hold up
  its ready state (e.g. by blocking the instantiate thread).  Call
  `serialize_state(10)` with a 10 ms timeout.  Verify it throws / returns
  error.
- [ ] **10.5** Test `deserialize_state` while the chain is not yet ready —
  the existing code waits in a sleep-loop.  Verify it blocks briefly and
  then works once the chain becomes ready.

**✓ Milestone check:** Full save/restore round-trip works for Carla
  internal state.

---

## M11 — Active / inactive and process correctness

- [ ] **11.1** Toggle the chain active/inactive (`set_fx_chain_active`)
  and observe audio output: when inactive, no audio should pass through;
  when active, it should.
- [ ] **11.2** Run a sustained processing load (60 s of audio through a
  Carla Rack) and verify no cumulative latency drift, no growing buffer
  backlog, no xruns.
- [ ] **11.3** Verify that `is_ready()` returns false while the async
  instantiation thread is still running, and true afterwards.
- [ ] **11.4** Call `stop()` on the chain; verify cleanup is orderly
  (no warnings about leaked handles, no crash on subsequent destroy).

**✓ Milestone check:** Active/inactive toggling and long-running process
  pass.

---

## M12 — Test the `Test2x2x1` chain path is unaffected

- [ ] **12.1** Confirm `BackendSession::create_fx_chain` still has the
  `case Test2x2x1:` branch that creates a `CustomProcessingChain` directly
  (not going through the double-bridge).
- [ ] **12.2** Create a Test2x2x1 chain and verify it processes audio and
  MIDI correctly (no regression).

**✓ Milestone check:** Test2x2x1 chain still works end-to-end.

---

## M13 — Run the full test suite

- [ ] **13.1** Run `cargo test --workspace` (or the equivalent
  project-specific test command).  Confirm all Rust-side tests pass.
- [ ] **13.2** Build and run the C++ test target (`src/backend/test/`).
  Confirm all backend tests pass — especially any tests that exercise
  FX chain creation, port connection, graph schedule recalc, and
  process.
- [ ] **13.3** Run the test suite under an LV2-enabled configuration
  (`SHOOP_HAVE_LV2` defined).  If there are LV2-specific tests, they
  must pass.
- [ ] **13.4** Run under Address Sanitizer (if available) for one
  full test cycle to catch use-after-free or double-free in the new
  port-pointer / no-op-deleter code.

**✓ Milestone check:** Full test suite passes with no regressions.

---

## M14 — Clean up & document

- [ ] **14.1** Remove any temporary hard-coded include paths from
  `CMakeLists.txt` that were used during development — all paths should
  flow through the `CARLA_LV2_CXX_INCLUDE` variable from Cargo.
- [ ] **14.2** Add a comment block at the top of `RustFxChainAdapter.h`
  explaining the double-bridge architecture, the no-op-deleter port
  lifetime model, and the future intent to move port ownership to Rust.
- [ ] **14.3** Add a similar comment block to `fx_chain.rs` and
  `CarlaLv2ChainCppFacade.h`.
- [ ] **14.4** Update `INSTALL.md` or project-specific build docs to
  mention the new crate and any new build requirements (if any).
- [ ] **14.5** Ensure `cargo clean && cargo build --workspace` still
  works from scratch (the build order must be: `carla_lv2` → emit env
  vars → CMake build reads them → backend links them).

**✓ Milestone check:** Clean build from scratch works.  Documentation
  is in place.
