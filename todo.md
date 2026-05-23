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

## Phase 9: BasicLoop C++ Wrapper Integration (PARTIALLY DONE)

BasicLoop now delegates to Rust core, but AudioMidiLoop integration has issues.

- [x] Create BasicLoop state accessor methods in Rust (for AudioMidiLoopCore to use):
  - [x] `get_next_poi()` - returns Option<PointOfInterest>
  - [x] `get_next_trigger()` - returns Option<u32>
  - [x] `get_maybe_next_planned_mode()` - returns LoopMode
  - [x] `get_maybe_next_planned_delay()` - returns i32
  - [x] `set_next_poi(poi: Option<PointOfInterest>)` - for merging channel POIs
- [x] Add accessor methods to basic_loop_cxx.rs bridge
- [x] Modify `src/backend/internal/BasicLoop.h`:
  - [x] Add `#include "backend_rust/src/basic_loop_cxx.rs.h"`
  - [x] Add `rust::Box<backend_rust::BasicLoopCore> m_rust_core` member
  - [x] Remove old state fields (mp_*, ma_* atomics) (kept sync_source as C++ managed)
  - [x] Keep LoopInterface inheritance and logging mixin
  - [x] Keep CommandQueue member
  - [x] Add helper methods for AudioMidiLoop to access internal state
- [x] Modify `src/backend/internal/BasicLoop.cpp`:
  - [x] Update constructor to create Rust core
  - [x] Implement all methods as delegates via cxx bridge
  - [x] Handle sync source shared_ptr <-> usize conversion
  - [x] Handle CommandQueue integration (thread_safe parameter)
- [x] Run `cargo build` to verify BasicLoop compiles
- [x] Run C++ tests `[BasicLoop]` - 9 tests pass
- [x] Run C++ tests `[SyncedBasicLoops]` - 3 tests pass
- [ ] Run C++ tests `[AudioMidiLoop]` - some tests failing

### Issues Found:
- AudioMidiLoop tests failing due to channel POI handling issues
- The sync source trigger behavior needs to match original C++ logic (fixed in Rust)
- AudioMidiLoop::PROC_update_poi and PROC_handle_poi need further investigation

---

## Phase 10-15: Remaining Work

AudioMidiLoop tests are failing. Investigation needed:
- Channel POI merging in AudioMidiLoop::PROC_update_poi
- Channel handling in AudioMidiLoop::PROC_handle_poi
- Channel processing in AudioMidiLoop::PROC_process_channels

The remaining phases (10-15) for AudioMidiLoop Rust implementation can proceed
once the current integration issues are resolved.

- [ ] Debug and fix AudioMidiLoop test failures
- [ ] Complete Phase 10: AudioMidiLoop Rust Implementation
- [ ] Complete Phase 11: Channel FFI Helpers
- [ ] Complete Phase 12: AudioMidiLoop CXX Bridge
- [ ] Complete Phase 13: AudioMidiLoop C++ Wrapper
- [ ] Complete Phase 14: Integration Testing
- [ ] Complete Phase 15: Final Cleanup