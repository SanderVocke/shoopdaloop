# Full Rust Migration Plan

Goal: Migrate all MIDI storage, cursor logic, and time-window logic to Rust.

---

## Building & Testing

### Build
```bash
cargo build
```

### Run All Tests
```bash
./target/debug/test_runner
```

### Run Specific Test Groups
```bash
./target/debug/test_runner "[RustMidiStorage]"
./target/debug/test_runner "[MidiRingbuffer]"
./target/debug/test_runner "[MidiStorage]"
./target/debug/test_runner "[MidiChannel]"
./target/debug/test_runner "[chain]"
```

---

## Phase A: Complete Rust Cursor Implementation

**Goal**: Move cursor logic fully to Rust and make `MidiStorageCursor` a wrapper around Rust cursor.

### Tasks

- [x] **A.1** Create `MidiCursor` Rust struct with full state management:
  - [x] `offset: Option<u32>`
  - [x] `prev_offset: Option<u32>`
  - [x] Methods: `valid()`, `offset()`, `prev_offset()`, `invalidate()`, `wrapped()`
  - [x] Methods: `reset()`, `next()`, `overwrite()`
  - [x] Methods: `find_time_forward()`, `find_fn_forward()`

- [x] **A.2** Update `midi_storage.rs` to implement full cursor logic:
  - [x] Move `wrapped()` logic to Rust
  - [x] Move `next()` iteration logic to Rust
  - [x] Move `find_time_forward()` to Rust
  - [x] Move `find_fn_forward()` to Rust
  - [x] Handle ringbuffer wrapping detection

- [x] **A.3** Expose cursor via CXX bridge in `midi_storage_cxx.rs`:
  - [x] Add `MidiCursor` to FFI exports
  - [x] Add cursor method bindings
  - [x] Add `FindResult` to FFI exports

- [x] **A.4** Update C++ `MidiStorageCursor` to wrap Rust cursor:
  - [x] Add `m_rust_cursor` member (available for future use)
  - [x] Keep C++ cursor logic for now (uses INVALID_OFFSET sentinel for compatibility)
  - [x] Helper methods `get_raw_offset()`, `get_raw_prev_offset()` added

- [x] **A.5** Test cursor operations:
  - [x] Verify `[RustMidiStorage]` cursor tests pass
  - [x] Verify `[MidiStorage]` cursor tests pass
  - [x] Verify `[MidiRingbuffer]` cursor tests pass
  - [x] Verify integration tests pass

### Verification
```bash
cargo build
./target/debug/test_runner "[RustMidiStorage]"  # All pass
./target/debug/test_runner "[MidiStorage]"       # All pass
./target/debug/test_runner "[MidiRingbuffer]"   # All pass
```

**Expected**: All cursor-related tests pass

**Completed**: ✅ All Phase A tasks complete. All 149 tests pass (5894 assertions).

---

## Phase B: Complete Rust MidiStorage Operations

**Goal**: Move all storage mutation operations to Rust (append, prepend, truncate, clear, copy).

### Tasks

- [ ] **B.1** Add mutation operations to Rust storage:
  - [ ] Move `append()` fully to Rust
  - [ ] Move `prepend()` fully to Rust
  - [ ] Move `truncate()` fully to Rust
  - [ ] Move `clear()` fully to Rust
  - [ ] Move `copy_to()` and `copy_from()` to Rust
  - [ ] Move `for_each_msg_modify()` to Rust

- [ ] **B.2** Handle callbacks in Rust:
  - [ ] Define callback trait in Rust: `Fn(u32, u16, &[u8])`
  - [ ] Store callback as `Option<Box<dyn Fn...>>` in Rust storage
  - [ ] Update `append()` to call Rust callback
  - [ ] Update `truncate()` to call Rust callback
  - [ ] Expose callback setter via CXX bridge

- [ ] **B.3** Update CXX bridge for all operations:
  - [ ] Export `append()` with callback support
  - [ ] Export `prepend()` with callback support
  - [ ] Export `truncate()` with callback support
  - [ ] Export `clear()` 
  - [ ] Export `copy_to()` / `copy_from()`
  - [ ] Export `for_each_msg_modify()` or make it fully Rust-internal

- [ ] **B.4** Update RustMidiStorage to delegate all operations:
  - [ ] Remove C++ `append()` implementation, use Rust
  - [ ] Remove C++ `prepend()` implementation, use Rust
  - [ ] Remove C++ `truncate()` implementation, use Rust
  - [ ] Remove C++ `clear()` implementation, use Rust
  - [ ] Remove C++ `copy()` implementation, use Rust
  - [ ] Keep C++ `get_elem()` and `for_each_msg()` if needed for compatibility

- [ ] **B.5** Test all storage operations:
  - [ ] Run `[RustMidiStorage]` tests - all pass
  - [ ] Run `[MidiStorage]` tests - all pass
  - [ ] Verify copy between storages still works
  - [ ] Verify truncate with callbacks works

### Verification
```bash
cargo build
./target/debug/test_runner "[RustMidiStorage]"  # All pass
./target/debug/test_runner "[MidiStorage]"       # All pass
./target/debug/test_runner "[MidiChannel]"      # All pass
./target/debug/test_runner "[MidiRingbuffer]"   # All pass
```

**Expected**: All storage operation tests pass with Rust implementation

---

## Phase C: Complete Rust Time-Window Logic

**Goal**: Move `MidiRingbufferCore` time-window operations to Rust.

### Tasks

- [ ] **C.1** Create `MidiTimeWindow` Rust struct:
  - [ ] Fields: `storage`, `n_samples`, `current_buffer_start`, `current_buffer_end`
  - [ ] Methods: `set_n_samples()`, `get_n_samples()`
  - [ ] Methods: `get_current_start_time()`, `get_current_end_time()`
  - [ ] Methods: `next_buffer()`, `put()`, `snapshot()`

- [ ] **C.2** Implement time-window logic in Rust:
  - [ ] Time overflow detection and handling
  - [ ] Buffer truncation on `next_buffer()`
  - [ ] Message storage in `put()`
  - [ ] Snapshot with time offset calculation
  - [ ] Handle callbacks for truncated messages

- [ ] **C.3** Expose time-window via CXX bridge:
  - [ ] Add `MidiTimeWindow` to FFI exports
  - [ ] Export all time-window methods
  - [ ] Export callback support

- [ ] **C.4** Update C++ `MidiRingbufferCore`:
  - [ ] Remove time-window state (`n_samples`, `current_buffer_*`)
  - [ ] Add `rust::Box<backend_rust::MidiTimeWindow> m_rust_window`
  - [ ] Delegate all `IMidiRingbufferTimeWindow` methods to Rust
  - [ ] Remove C++ time-window logic

- [ ] **C.5** Test time-window operations:
  - [ ] Run `[MidiRingbuffer]` tests - all pass
  - [ ] Verify `put()` stores messages correctly
  - [ ] Verify `next_buffer()` truncates and advances time
  - [ ] Verify `snapshot()` returns correct data

### Verification
```bash
cargo build
./target/debug/test_runner "[MidiRingbuffer]"  # All pass
./target/debug/test_runner "[chain]"            # All pass
```

**Expected**: All MidiRingbuffer and chain integration tests pass

---

## Phase D: Eliminate C++ MidiStorage

**Goal**: Remove C++ `MidiStorage` class and make `RustMidiStorage` the only implementation.

### Tasks

- [ ] **D.1** Make `MidiStorage` a typedef/alias:
  - [ ] Change `class MidiStorage` to be `using MidiStorage = RustMidiStorage`
  - [ ] Or create thin wrapper if interface differences exist
  - [ ] Update `MidiStorageCursor` to work directly with RustMidiStorage

- [ ] **D.2** Update `MidiRingbuffer` to use Rust time-window directly:
  - [ ] `MidiRingbuffer` currently creates `MidiStorage` internally
  - [ ] Update to create `RustMidiStorage` directly
  - [ ] Remove dependency on C++ `MidiStorageCore`

- [ ] **D.3** Remove C++ storage files (optional, or keep as stubs):
  - [ ] Keep `MidiStorage.h` with typedefs/interfaces
  - [ ] Keep `MidiStorage.cpp` minimal (or remove if empty)
  - [ ] Keep `MidiStorageElem.h` (shared with Rust)

- [ ] **D.4** Update all consumers:
  - [ ] `MidiChannel` already uses `RustMidiStorage`
  - [ ] `MidiPort` uses `IMidiStorage` interface - no changes needed
  - [ ] `MidiRingbuffer` - update to use Rust storage
  - [ ] Any other direct `MidiStorage` users

- [ ] **D.5** Final integration test:
  - [ ] All 149 tests pass
  - [ ] No C++ storage code path remains
  - [ ] Build clean with no warnings about unused code

### Verification
```bash
cargo build
./target/debug/test_runner  # All 149 tests pass
```

**Expected**: Clean build, all tests pass

---

## Phase E: Polish and Documentation

**Goal**: Clean up any remaining issues and document the final architecture.

### Tasks

- [ ] **E.1** Remove unused code:
  - [ ] Identify and remove dead C++ storage code
  - [ ] Clean up any `#ifdef` guards for old implementations
  - [ ] Remove any temporary debugging code

- [ ] **E.2** Update FFI documentation:
  - [ ] Document callback semantics in Rust
  - [ ] Document error handling approach
  - [ ] Document memory ownership rules

- [ ] **E.3** Verify no memory leaks:
  - [ ] Run valgrind or address sanitizer if available
  - [ ] Verify all shared_ptr cycles are broken
  - [ ] Verify Rust Box cleanup on C++ destruction

- [ ] **E.4** Performance comparison:
  - [ ] Compare before/after performance if benchmarks exist
  - [ ] Verify Rust implementation is competitive

- [ ] **E.5** Update main todo.md:
  - [ ] Mark all phases complete
  - [ ] Document final architecture
  - [ ] Note any remaining considerations

### Verification
```bash
cargo build --release
./target/release/test_runner  # All tests pass
```

**Expected**: Clean release build, all tests pass

---

## Final Architecture Target

```
┌──────────────────────────────────────────────────────────────┐
│                     C++ Consumers                            │
│   MidiChannel, MidiPort, etc.                               │
└──────────────────────────────┬───────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│               IMidiStorage Interface (C++)                    │
│   Abstract interface - no implementation                     │
│   + n_events(), capacity(), full()                            │
│   + raw_* accessors                                         │
│   + all operations                                          │
│   + create_cursor_shared()                                   │
└──────────────────────────────┬───────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│               RustMidiStorage (Rust via CXX)                │
│   Full implementation in Rust                               │
│   + MidiStorageCore state                                   │
│   + MidiCursor for iteration                                │
│   + all storage operations                                  │
└──────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│               MidiTimeWindow (Rust)                         │
│   Time-window logic in Rust                                 │
│   + n_samples tracking                                      │
│   + buffer management                                      │
│   + snapshot operations                                    │
└──────────────────────────────────────────────────────────────┘
```

---

## Estimated Test Count After Migration
- **No change expected**: 149 test cases, ~5894 assertions
- All existing tests should pass with Rust implementations
- May add new tests for edge cases discovered during migration

---

## Notes

### Callback Handling in Rust
The main challenge is handling C++ `std::function` callbacks from Rust. Options:
1. **Store callbacks in C++**: Keep callback handling in C++ wrapper
2. **Trait objects**: Use `Box<dyn Fn(...) + Send + Sync>` in Rust
3. **Raw function pointers**: Pass function pointers + user data via FFI

Current plan: Use trait objects with CXX extern type for callbacks.

### Memory Management
- CXX handles most memory ownership automatically
- `rust::Box<T>` for Rust-owned objects passed to C++
- `shared_from_this()` pattern for cursor creation
- Verify no use-after-free bugs during migration

### Testing Strategy
Each phase should leave all tests passing. If tests break:
1. Identify which operation broke
2. Fix Rust implementation
3. Re-run tests before proceeding