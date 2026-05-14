# MIDI Storage & Ringbuffer Refactoring Plan

Goal: Refactor C++ MIDI storage classes to enable clean port to Rust using CXX bridges.

---

## Building & Testing

### Build
```bash
cargo build
```

This builds the application and creates a `test_runner` executable in the `target/` directory.

### Run C++ Unit Tests
```bash
./target/debug/test_runner
```
Or with release:
```bash
./target/release/test_runner
```

Run specific tests by name:
```bash
./target/debug/test_runner "[MidiStorage]"
```

---

## Phase 1: Flatten inheritance into composition (COMPLETED)

**Goal**: Replace is-a inheritance with has-a composition. This maps better to Rust traits and eliminates the awkward `using Storage = MidiStorage` aliasing.

### Tasks
- [x] Create standalone `MidiStorageCore` class containing the raw ringbuffer data (`m_data`, `m_tail`, `m_head`, `m_n_events`) and basic operations (`append`, `prepend`, `copy`, `clear`, `full`, `n_events`, `bytes_*`)
- [x] Remove `MidiStorageBase` class entirely (merged into new `MidiStorageCore`)
- [x] Refactor `MidiStorage` to **contain** a `MidiStorageCore` via `std::unique_ptr<MidiStorageCore>` instead of inheriting
- [x] Add cursor management methods to `MidiStorage` that delegate to `MidiStorageCore`
- [x] Refactor `MidiRingbuffer` to **contain** a `MidiStorage` instead of inheriting
- [x] Update all includes and constructor calls in consumers
- [x] Verify all tests pass

### Build & Test
```bash
cargo build
./target/debug/test_runner "[MidiStorage]"
./target/debug/test_runner "[MidiRingbuffer]"
```

✅ All 140 test cases pass

---

## Phase 2: Extract IMidiStorageCore interface (COMPLETED)

**Goal**: Define a pure virtual interface that C++ consumers can use and Rust can implement/bridge to.

### Tasks
- [x] Create `IMidiStorageCore` interface with pure virtual methods:
  - `n_events() const`
  - `get_elem(uint32_t idx)` → mutable elem access
  - `get_elem(uint32_t idx) const` → immutable elem access
  - `raw_tail()`, `raw_head()`, `raw_capacity()`, `raw_full()`
- [x] Create `IMidiStorageOperations` interface:
  - `append(time, size, data, allow_replace, dropped_cb)`
  - `clear()`
  - `copy_to(IMidiStorageCore& target)`
  - `truncate(time, side, dropped_cb)`
  - `for_each_msg_modify(callback)`
- [x] Make `MidiStorageCore` implement these interfaces
- [x] Update consumers to use interfaces where appropriate (not required for all consumers, just for the Rust bridge boundary)

### Build & Test
```bash
cargo build
./target/debug/test_runner "[MidiStorage]"
./target/debug/test_runner "[MidiRingbuffer]"
```

✅ All 140 test cases pass

**Files created/modified:**
- `IMidiStorageCore.h` - Pure virtual interfaces for FFI
- `MidiStorageElem.h` - Extracted element struct with inline implementations
- `MidiStorage.h` - MidiStorageCore now implements the interfaces
- `MidiStorage.cpp` - Updated copy() methods to use interface

---

## Phase 3: Extract cursor interface (COMPLETED)

**Goal**: Define cursor operations as free functions working on interfaces, enabling Rust cursor implementation.

### Tasks
- [x] Create `IMidiStorageCursor` interface:
  - `valid()`, `offset()`, `invalidate()`, `reset()`
  - `get()`, `get_prev()`, `next()`
  - `wrapped()`, `is_at_start()`
  - `overwrite(offset, prev_offset)`
- [x] Create `cursor_find_time_forward()` free function
- [x] Create `cursor_find_fn_forward()` free function
- [x] Refactor `MidiStorageCursor` to implement `IMidiStorageCursor`
- [x] Ensure cursor can be created from storage interface

### Build & Test
```bash
cargo build
./target/debug/test_runner "[MidiStorage]"
./target/debug/test_runner "[MidiRingbuffer]"
```

✅ All 140 test cases pass

**Files created/modified:**
- `IMidiStorageCursor.h` - Pure virtual interface for cursor operations
- `IMidiStorageCursor.cpp` - Free function implementations
- `MidiStorage.h` - MidiStorageCursor now implements IMidiStorageCursor
- `MidiStorage.cpp` - Updated to use interface FindResult type

---

## Phase 4: Extract time-window logic from MidiRingbuffer (COMPLETED)

**Goal**: Isolate the time-window logic (n_samples tracking, next_buffer, put, snapshot) so it can be easily bridged to Rust independently of the storage core.

### Tasks
- [x] Create `IMidiRingbufferTimeWindow` interface:
  - `set_n_samples(n)`
  - `get_n_samples() const`
  - `next_buffer(n_frames, dropped_cb)`
  - `put(frame_in_buffer, size, data, dropped_cb) -> bool`
  - `snapshot(target, start_offset_from_end)`
  - `get_current_start_time()`, `get_current_end_time()`
- [x] Make `MidiRingbufferCore` implement this interface
- [x] Refactor `MidiRingbuffer` to contain `MidiRingbufferCore` instead of inheriting from `MidiStorage`

### Build & Test
```bash
cargo build
./target/debug/test_runner "[MidiStorage]"
./target/debug/test_runner "[MidiRingbuffer]"
```

✅ All 140 test cases pass

**Files created/modified:**
- `IMidiRingbufferTimeWindow.h` - Pure virtual interface for time-window operations
- `MidiStorage.h` - Added MidiRingbufferCore class, updated MidiStorage to implement IMidiStorage
- `MidiStorage.cpp` - Added MidiRingbufferCore implementation, updated copy() to work with interface

---

## Phase 5: Clean up remaining integration points

**Goal**: Ensure the bridge boundary is clean before Rust port begins.

### Tasks
- [ ] Remove any remaining inheritance patterns in MIDI storage classes
- [ ] Verify no raw pointer arithmetic that could cause FFI issues
- [ ] Ensure all `std::function` callbacks at the bridge boundary are documented
- [ ] Add `dropped_msg_cb` parameter consistently across all mutating operations
- [ ] Update tests to use refactored classes
- [ ] All tests pass

### Build & Test
```bash
cargo build
./target/debug/test_runner   # Run all tests to ensure nothing is broken
```

---

## Phase 6: Add Rust storage implementation with CXX bridge

**Goal**: Implement MIDI storage logic in Rust and expose via CXX.

### Tasks
- [ ] Create `midi_storage.rs` with `MidiStorageCore` struct
- [ ] Create `midi_storage_cxx.rs` with CXX bridge
- [ ] Implement `MidiStorageElem` serialization if needed
- [ ] Implement cursor logic in Rust
- [ ] Implement time-window logic in Rust
- [ ] Add C++ wrapper classes that delegate to Rust
- [ ] Verify all tests pass with Rust implementation

### Build & Test
```bash
cargo build
./target/debug/test_runner   # All tests should pass with Rust implementation
```

---

## Phase 7: Remove C++ storage implementations (optional)

**Goal**: After Rust implementation is validated, remove C++ storage code.

### Tasks
- [ ] Verify Rust implementation passes all tests
- [ ] Remove `MidiStorageCore.cpp`, `MidiStorage.cpp` (keep headers with forwarding/wrapper classes)
- [ ] Remove `MidiRingbuffer.cpp` (keep headers with forwarding/wrapper classes)
- [ ] Update any remaining C++ consumers to use Rust-backed classes
- [ ] Final test suite passes

### Build & Test
```bash
cargo build
./target/debug/test_runner   # All tests should pass
```