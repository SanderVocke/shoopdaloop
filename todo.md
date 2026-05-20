# TODO: Port DecoupledMidiPort to Rust

## Phase 1 — Rust Core and Bridge
- [x] Create `src/rust/backend_rust/src/decoupled_midi_port.rs` with `DecoupledMidiPort` struct and queue logic
- [x] Create `src/rust/backend_rust/src/decoupled_midi_port_cxx.rs` with CXX bridge exposing `new_decoupled_midi_port`, `decoupled_push`, `decoupled_pop`, `decoupled_is_empty`
  - *Note: Used named parameters (`port: &DecoupledMidiPort`) instead of `self` syntax due to a CXX macro issue where `self` generated an unresolved method lookup. C++ side calls them as free functions (`backend_rust::decoupled_push(*m_rust, ...)`).*
- [x] Register both modules in `src/rust/backend_rust/src/lib.rs`
- [x] Register the CXX bridge in `src/rust/backend_rust/build.rs`
- [x] `cargo build` compiles successfully
- [x] `cargo test` passes (including any new Rust unit tests)

## Phase 2 — C++ Wrapper Refactor
- [x] Update `src/backend/internal/DecoupledMidiPort.h`: remove template parameters, replace `boost::lockfree::spsc_queue` with `rust::Box<backend_rust::DecoupledMidiPort>`, remove `extern template` declarations
- [x] Update `src/backend/internal/DecoupledMidiPort.cpp`: de-template all methods, replace queue push/pop with Rust bridge calls, remove explicit template instantiations at bottom
- [x] Update `src/backend/internal/shoop_globals.h`: change `DecoupledMidiPort` forward declaration from template to concrete class, update `_DecoupledMidiPort` alias
- [x] `cargo build` compiles successfully (C++ side consumes new bridge)

## Phase 3 — Integration and Cleanup
- [x] Verify `AudioMidiDriver.h`, `AudioMidiDriver.cpp`, `libshoopdaloop_backend.cpp`, and `libshoopdaloop_test_if.h` require no changes (they use `_DecoupledMidiPort` alias)
- [x] Remove any unused `#include <boost/lockfree/spsc_queue.hpp>` from `DecoupledMidiPort.h`
- [x] Run `cargo fmt --all`
- [x] `RUSTFLAGS="-D warnings" cargo build` succeeds with zero warnings

## Phase 4 — Testing and Validation
- [x] `cargo test` passes
- [x] C++ `test_runner` passes (find under `target/debug/**/test_runner`)
- [x] QML self-tests pass: `./target/debug/shoopdaloop_dev.sh --self-test`
- [x] Final review: no original C++ queue logic remains, no compiler warnings, all tests green
