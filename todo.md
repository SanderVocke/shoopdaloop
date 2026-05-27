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

Refined execution sequence for full facade removal:
- [x] Pass 1: Introduce a shared direct-Rust queue helper API for C++ callsites (new header/cpp), while keeping existing `CommandQueue` facade intact
- [x] Pass 1 milestone: build and run `cargo test` + backend `test_runner`
- [~] Pass 2: Migrate non-templated core classes to direct Rust queue ownership (`rust::Box<backend_rust::CommandQueue>`) and remove direct `CommandQueue` type dependencies in those classes (BackendSession migrated; AudioMidiDriver intentionally deferred to Pass 3 due broad derived/template dependency surface)
- [x] Pass 2 milestone: build and run backend `test_runner`
- [ ] Pass 3: Migrate templated/internal utility classes (`AudioChannel`, `BufferQueue`, etc.) and remaining callsites
- [ ] Pass 3 milestone: build and run `cargo test` + backend `test_runner`
- [ ] Pass 4: Remove legacy C++ `CommandQueue.h/.cpp` and update all includes/references
- [ ] Pass 4 milestone: full build with `RUSTFLAGS="-D warnings" cargo build`
- [ ] Pass 5: Add/extend tests for clear/drop cleanup, queue-and-wait, passthrough, inactivity fallback with C++-enqueued commands
- [ ] Pass 5 milestone: run backend `test_runner`
- [ ] Pass 6: Documentation cleanup for ownership/threading semantics and stale callback/SPSC wording
- [ ] Pass 6 milestone: `cargo fmt --all`, `cargo test`, backend `test_runner`
- [ ] Final milestone: run `./target/debug/shoopdaloop_dev.sh --self-test` (if environment allows)
- [ ] Confirm no warnings, all tests passing, and migration complete

Status note:
- QML self-test previously aborted in this environment due to crash-handling startup (`Connection refused` then abort). If this persists, report as environment/runtime issue after all code-level milestones pass.