- [ ] Confirm current baseline: `cargo build`, `cargo test`, backend `test_runner`
- [x] Add/extend Rust runtime type for `AudioMidiDriver` orchestration in `src/rust/backend_rust/src/`
- [x] Add CXX bridge API for Rust runtime process-cycle entrypoint and handle registration/removal
- [x] Add C++ trampoline functions callable from Rust for:
  - [x] `HasAudioProcessingFunction::PROC_process`
  - [x] `DecoupledMidiPort::PROC_process`
  - [x] process callback thunk and command queue exec thunk
- [x] Refactor `AudioMidiDriver::PROC_process` to thin Rust-forwarding wrapper
- [x] Forward processor and decoupled-port bookkeeping to Rust runtime handle sets while preserving C++ API
- [x] Build/test milestone: `cargo build`, `cargo test`, backend `test_runner`
  - [x] `cargo build`
  - [x] `cargo test`
  - [x] backend `test_runner`
  - nuance: C++ still keeps `m_processors` weak-list for `processors()` API compatibility; process-cycle execution path now runs through Rust runtime callbacks.
  - nuance: CXX trampoline symbol definitions were placed in `AudioMidiDriver.cpp` (with dedicated header) to avoid link visibility/order issues for backend test targets.
  - nuance: `cargo test` reported existing toolchain warning (`gold linker is deprecated`), not introduced by this change.

- [ ] Move dummy processing thread/timing loop from C++ to Rust runtime (`DummyAudioMidiDriver` control object)
- [ ] Keep C++ `DummyAudioMidiDriver` public methods/signatures intact; delegate to Rust
- [ ] Preserve pause/resume/finish/controlled-mode semantics and wait behavior
- [ ] Build/test milestone: `cargo build`, `cargo test`, backend `test_runner`

- [ ] Keep C++ port object creation/return types unchanged (`DummyAudioPort`, `DummyMidiPort`)
- [ ] Migrate only internal driver bookkeeping to Rust where needed via opaque handles
- [ ] Build/test milestone: `cargo build`, backend `test_runner`

- [ ] Migrate decoupled MIDI registration ownership/bookkeeping to Rust runtime
- [ ] Verify unregister/close lifecycle safety with command queue thread constraints
- [ ] Build/test milestone: `cargo build`, `cargo test`, backend `test_runner`

- [ ] Thin-wrapper cleanup pass in C++ (`AudioMidiDriver*`, `DummyAudioMidiDriver*`)
- [ ] Update comments/docs for new Rust-vs-C++ responsibility split

- [ ] Final formatting and strict warning gate: `cargo fmt --all` then `RUSTFLAGS="-D warnings" cargo build`
- [ ] Final tests: `cargo test`, backend `test_runner`, `./target/debug/shoopdaloop_dev.sh --self-test` (if environment supports it)
- [ ] Confirm final state: all tests passing, no warnings, C++ wrappers thin, behavior preserved
