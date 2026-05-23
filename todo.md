# TODO: Port BasicLoop and AudioMidiLoop to Rust

## Phase 1-8: BasicLoop Rust Implementation (DONE)

All phases completed. BasicLoopCore implemented in `basic_loop.rs` with CXX bridge in `basic_loop_cxx.rs`.
- [x] Phase 1: Core Rust struct and types (LoopMode, PointOfInterest, BasicLoopCore)
- [x] Phase 2: Basic getters/setters (position, length, mode)
- [x] Phase 3: POI logic (proc_get_next_poi, proc_update_poi, dominant_poi)
- [x] Phase 4: Trigger and transition logic
- [x] Phase 5: Planned transitions queue
- [x] Phase 6: Process method
- [x] Phase 7: Sync source handling
- [x] Phase 8: CXX bridge
- [x] 18 Rust unit tests passing
- [x] Cargo build with no warnings

---

## Phase 9: BasicLoop C++ Wrapper Integration

Now that AudioMidiLoop will be ported to Rust, BasicLoop wrapper can proceed.

- [ ] Create BasicLoop state accessor methods in Rust (for AudioMidiLoopCore to use):
  - [ ] `get_next_poi()` - returns Option<PointOfInterest>
  - [ ] `get_next_trigger()` - returns Option<u32>
  - [ ] `get_maybe_next_planned_mode()` - returns LoopMode
  - [ ] `get_maybe_next_planned_delay()` - returns i32
  - [ ] `set_next_poi(poi: Option<PointOfInterest>)` - for merging channel POIs
- [ ] Add accessor methods to basic_loop_cxx.rs bridge
- [ ] Modify `src/backend/internal/BasicLoop.h`:
  - [ ] Add `#include "backend_rust/src/basic_loop_cxx.rs.h"`
  - [ ] Add `rust::Box<backend_rust::BasicLoopCore> m_rust_core` member
  - [ ] Remove old state fields (mp_*, ma_* atomics)
  - [ ] Keep LoopInterface inheritance and logging mixin
  - [ ] Keep CommandQueue member
  - [ ] Declare methods as delegates to Rust
- [ ] Modify `src/backend/internal/BasicLoop.cpp`:
  - [ ] Update constructor to create Rust core
  - [ ] Implement all methods as delegates via cxx bridge
  - [ ] Handle sync source shared_ptr <-> usize conversion
  - [ ] Handle CommandQueue integration (thread_safe parameter)
- [ ] Run `cargo build` to verify BasicLoop compiles
- [ ] Run `./target/debug/test_runner "[BasicLoop]"` - verify tests pass

---

## Phase 10: AudioMidiLoop Rust Implementation

Create AudioMidiLoopCore in Rust with channel support via FFI.

- [ ] Create `src/rust/backend_rust/src/audio_midi_loop.rs`:
  - [ ] `AudioMidiLoopCore` struct with:
    - [ ] All BasicLoopCore fields (or BasicLoopCore instance)
    - [ ] `audio_channels: Vec<usize>` (opaque pointers to C++ AudioChannel)
    - [ ] `midi_channels: Vec<usize>` (opaque pointers to C++ MidiChannel)
  - [ ] `new_audio_midi_loop_core()` constructor
  - [ ] Channel management methods:
    - [ ] `add_audio_channel(ptr: usize)` - stores pointer
    - [ ] `add_midi_channel(ptr: usize)` - stores pointer  
    - [ ] `audio_channel(idx: usize) -> usize` - retrieves pointer
    - [ ] `midi_channel(idx: usize) -> usize` - retrieves pointer
    - [ ] `delete_audio_channel(ptr: usize)` - removes from vector
    - [ ] `delete_midi_channel(ptr: usize)` - removes from vector
    - [ ] `n_audio_channels() -> u32`
    - [ ] `n_midi_channels() -> u32`
  - [ ] Override methods (call BasicLoopCore methods + add channel logic):
    - [ ] `proc_update_poi()` - calls channels via FFI, merges POIs
    - [ ] `proc_handle_poi()` - calls channels' handle_poi via FFI
    - [ ] `proc_process_channels()` - calls channels' process via FFI
- [ ] Add `pub mod audio_midi_loop;` to `lib.rs`
- [ ] Add Rust unit tests for AudioMidiLoopCore
- [ ] Run `cargo test audio_midi_loop` to verify

---

## Phase 11: Channel FFI Helpers (C++)

Create C++ extern "C" functions for Rust to call channel methods.

- [ ] Create `src/backend/internal/channel_ffi_helpers.h/cpp` (or add to existing file):
  - [ ] `extern "C" uint32_t channel_proc_get_next_poi(usize ptr, u32 mode, u32 next_mode, i32 next_delay, u32 next_eta, u32 length, u32 position)`
  - [ ] `extern "C" void channel_proc_handle_poi(usize ptr, u32 mode, u32 length, u32 position)`
  - [ ] `extern "C" void channel_proc_process(usize ptr, u32 mode, u32 next_mode, i32 next_delay, u32 next_eta, u32 n_samples, u32 pos_before, u32 pos_after, u32 length_before, u32 length_after)`
  - [ ] Each function casts usize to ChannelInterface* and calls appropriate method
- [ ] Update CMakeLists.txt if needed for new source files
- [ ] Run `cargo build` to verify compilation

---

## Phase 12: AudioMidiLoop CXX Bridge

Create CXX bridge exposing AudioMidiLoopCore to C++.

- [ ] Create `src/rust/backend_rust/src/audio_midi_loop_cxx.rs`:
  - [ ] `#[cxx::bridge]` module with all method signatures
  - [ ] Expose `AudioMidiLoopCore` type
  - [ ] Channel management functions (add/delete/get/count)
  - [ ] Process thread functions
  - [ ] Declare extern "C" FFI functions for channel operations
- [ ] Add `pub mod audio_midi_loop_cxx;` to `lib.rs`
- [ ] Add to `build.rs` for CXX header generation
- [ ] Run `cargo build` to verify CXX bridge compiles
- [ ] Run `RUSTFLAGS="-D warnings" cargo build` - check for warnings

---

## Phase 13: AudioMidiLoop C++ Wrapper

Modify AudioMidiLoop to delegate to Rust.

- [ ] Modify `src/backend/internal/AudioMidiLoop.h`:
  - [ ] Add `#include "backend_rust/src/audio_midi_loop_cxx.rs.h"`
  - [ ] Add `rust::Box<backend_rust::AudioMidiLoopCore> m_rust_core` member
  - [ ] Remove channel vectors (now in Rust)
  - [ ] Keep inherited BasicLoop methods (via BasicLoop wrapper)
  - [ ] Declare methods as delegates to Rust
- [ ] Modify `src/backend/internal/AudioMidiLoop.cpp`:
  - [ ] Update constructor to create Rust core
  - [ ] Implement channel add methods:
    - [ ] Create AudioChannel/MidiChannel in C++
    - [ ] Pass pointer to Rust via cxx bridge
  - [ ] Implement channel get methods:
    - [ ] Get pointer from Rust
    - [ ] Cast to correct type, return shared_ptr
  - [ ] Implement channel delete methods:
    - [ ] Remove pointer from Rust
    - [ ] Delete C++ object
  - [ ] Implement count methods as delegates
  - [ ] Process methods delegate to Rust
- [ ] Run `cargo build` to compile full project
- [ ] Run `./target/debug/test_runner "[BasicLoop]"` - verify still pass
- [ ] Run `./target/debug/test_runner "[AudioMidiLoop]"` - verify pass

---

## Phase 14: Integration Testing

Verify all components work together.

- [ ] Run `./target/debug/test_runner "[SyncedBasicLoops]"` - integration tests
- [ ] Run `./target/debug/test_runner "[AudioMidiLoop][audio]"` - audio channel tests
- [ ] Run `./target/debug/test_runner "[AudioMidiLoop][midi]"` - midi channel tests
- [ ] Run full test suite: `./target/debug/test_runner`
- [ ] Verify all tests pass

---

## Phase 15: Final Cleanup

- [ ] Run `cargo fmt --all`
- [ ] Run `RUSTFLAGS="-D warnings" cargo build` - must have no warnings
- [ ] Verify project builds cleanly
- [ ] Delete plan.md and todo.md (task complete)