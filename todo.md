# TODO: remove C++ processor mirror list from AudioMidiDriver

- [x] Orientation
  - [x] Review `plan.md`
  - [x] Review `AudioMidiDriver.h/.cpp` processor add/remove/processors implementation
  - [x] Review Rust `audio_midi_driver.rs` processor registration structures and methods
  - [x] Review `audio_midi_driver_cxx.rs` CXX bridge patterns and existing exposed structs/methods
  - [x] Review `BridgeObject.h/.cpp` processor resolver helpers
  - [x] Search current uses of `processors()` and confirm expected compatibility surface

- [x] Phase 1: expose Rust-owned processor weak handles to C++
  - [x] Add a CXX-local POD struct in `audio_midi_driver_cxx.rs` for processor weak handles, e.g. `{ id: u64, type_id: u32 }`
  - [x] Add Rust `AudioMidiDriverCore` method to snapshot processor weak handles from Rust registrations (existing method reused)
  - [x] Expose that method through the CXX bridge
  - [x] Run `cargo build`

- [x] Phase 2: remove C++ `m_processors` mirror
  - [x] Remove `m_processors` member from `AudioMidiDriver`
  - [x] Update `AudioMidiDriver::add_processor` to stop pushing into a C++ vector
  - [x] Update `AudioMidiDriver::remove_processor` to stop erasing from a C++ vector
  - [x] Implement `AudioMidiDriver::processors()` by querying Rust weak handles and resolving each handle to a typed C++ processor pointer
  - [x] Skip stale/unresolvable handles defensively in `processors()`
  - [x] Confirm no C++ processor membership mirror list remains
  - [x] Run `cargo build`

- [x] Phase 3: tests
  - [x] Add or update backend tests for `processors()` empty state
  - [x] Add or update backend tests for `processors()` after adding one processor
  - [x] Add or update backend tests for `processors()` after removing a processor
  - [x] Add or update backend tests for multiple processors without assuming order
  - [x] Run backend `test_runner`
  - [x] Run `cargo test`

- [x] Phase 4: cleanup and final verification
  - [x] Confirm Rust owns processor bridge strong registration records
  - [x] Confirm no C++ processor bridge strong keepalive map was introduced
  - [x] Confirm no C++ processor membership vector/list mirror remains
  - [x] Confirm `AudioMidiDriver::processors()` reconstructs from Rust-owned weak handles
  - [x] Confirm process cycle still uses Rust processor registrations and typed bridge resolver pattern
  - [x] Clean comments and includes
  - [x] Run `cargo fmt --all`
  - [x] Run `RUSTFLAGS="-D warnings" cargo build`
  - [x] Run `cargo test`
  - [x] Run backend `test_runner`
  - [x] Run `QT_QPA_PLATFORM=offscreen ./target/debug/shoopdaloop_dev.sh --self-test` (first 120s attempt timed out; rerun with 300s passed)
  - [x] Fix all warnings/errors introduced by this task
  - [x] Confirm final state: no C++ processor mirror list, tests pass, strict build passes, formatting applied
