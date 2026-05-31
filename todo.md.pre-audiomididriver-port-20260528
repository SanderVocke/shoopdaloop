- [x] Read and understand current CommandQueue bridge and all C++ callsites
- [x] Pivoted implementation approach from `UniquePtr<CxxCommandToken>` to `usize` handle + explicit execute/destroy callbacks (required because `UniquePtr<opaque>` is not `Send`)
- [x] Implement Rust CXX bridge changes for `cxx_command_execute_ptr` / `cxx_command_destroy_ptr` and `cxx_queue*` handle APIs
- [x] Implement Rust command wrapper/drop semantics for exactly-once execute-or-destroy in `src/rust/backend_rust/src/command_queue.rs`
- [x] Add C++ command-handle helpers and FFI functions in `src/backend/internal/CommandToken.h/.cpp`
- [x] Migrate `src/backend/internal/CommandQueue.cpp` to handle-based bridge calls
- [x] Remove static callback registration use from C++ facade
- [x] Resolve CXX include-path/header visibility mismatch
- [x] Build milestone: run `cargo build` and fix compile issues
- [x] Build + test milestone: run `cargo test`
- [x] Build + test milestone: run backend `test_runner`

Migration insights applied:
- Base-first migration (e.g. `AudioMidiDriver`) causes large template-derived breakage.
- Safer path is helper-first call migration, then owner type migration, then facade deletion.
- Keep dual compatibility shims temporarily (e.g. overloaded helper use in API glue) until all owners are migrated.

Execution tracker (refined)
- [x] Pass 1: Introduce shared direct-Rust queue helper API (`RustCommandQueue.h`) while keeping facade
- [x] Pass 1 milestone: `cargo build`, `cargo test`, backend `test_runner`
- [~] Pass 2: Migrate non-templated core owners incrementally
  - [x] `BackendSession` migrated to direct `rust::Box<backend_rust::CommandQueue>` usage
  - [ ] `AudioMidiDriver` migration deferred until template-derived callsites are migrated in same pass set
- [x] Pass 2 milestone: build + backend `test_runner`

Next concrete steps to reach full Rust migration
- [~] Pass 3A: Migrate template/derived users to helper-call style (without changing owner type yet)
  - [x] `DummyAudioMidiDriver.cpp`
  - [x] `jack/JackAudioMidiDriver.cpp`
  - [ ] other template-heavy queue callsites
- [x] Pass 3A milestone: `cargo build`, backend `test_runner`
- [ ] Pass 3B: Migrate `AudioMidiDriver` owner type to direct Rust queue after Pass 3A callsite compatibility
- [ ] Pass 3B milestone: `cargo build`, `cargo test`, backend `test_runner`
- [ ] Pass 4: Migrate remaining queue owners (`AudioChannel`, `MidiChannel`, `BasicLoop`, `BufferQueue`, `Dummy*Port`, etc.) to direct Rust box ownership
- [ ] Pass 4 milestone: `cargo build`, `cargo test`, backend `test_runner`
- [ ] Pass 5: Remove all remaining dependencies on C++ facade API methods and includes
- [ ] Pass 6: Delete `src/backend/internal/CommandQueue.h/.cpp`, update build/source references
- [ ] Pass 6 milestone: `RUSTFLAGS="-D warnings" cargo build`
- [ ] Pass 7: Add/adjust tests for clear/drop cleanup and queue semantics through C++ enqueue path
- [ ] Pass 7 milestone: backend `test_runner`
- [ ] Pass 8: Documentation cleanup for ownership/threading semantics and stale comments
- [ ] Final: `cargo fmt --all`, `cargo test`, backend `test_runner`, and `./target/debug/shoopdaloop_dev.sh --self-test`
- [ ] Confirm no warnings, all tests passing, and migration complete

Environment note
- QML self-test may abort in this environment due to crash-handling startup (`Connection refused` then abort). If still present at final stage, report as runtime environment blocker with logs.