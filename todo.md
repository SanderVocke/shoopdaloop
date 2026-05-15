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
./target/debug/build/backend-35fc8be03c2c96e8/out/cmake_build/build/test/test_runner
```

---

## Progress Summary

| Phase | Description | Status |
|-------|-------------|--------|
| A | Rust Cursor Implementation | ✅ Complete |
| B | Rust MidiStorage Operations | ✅ Complete |
| C | Rust Time-Window Logic | ⬜ Not Started |
| D | Eliminate C++ MidiStorage | ⬜ Not Started |
| E | Polish and Documentation | ⬜ Not Started |

**Overall**: 2/5 phases complete. All 149 tests pass (5894 assertions).

---

## Phase A: Rust Cursor Implementation ✅

**Completed**: All cursor logic moved to Rust (`MidiCursor` in `midi_storage.rs`).

### What's Done
- `MidiCursor` struct with `valid()`, `offset()`, `prev_offset()`, `invalidate()`, `wrapped()`
- `reset()`, `next()`, `overwrite()`, `find_time_forward()`, `find_fn_forward()`
- Exposed via CXX bridge in `midi_storage_cxx.rs`

---

## Phase B: Rust MidiStorage Operations ✅

**Completed**: All mutation operations delegated to Rust.

### What's Done
- `append()` - returns success/dropped flags, use `get_last_dropped_elem()` for dropped message
- `truncate()` - preview-then-act pattern via `truncate_preview()` + `truncate_doit()`
- `clear()`, `copy_to()`, `copy_from()`
- Eliminated `std::function` callbacks entirely

### Key FFI Functions Added
```rust
// Append with dropped message handling
unsafe fn append(storage, time, size, data, allow_replace) -> u8  // flags
fn get_last_dropped_elem(storage) -> u32  // index of dropped element

// Truncate preview-then-act pattern
fn truncate_preview(storage, time, side) -> u32  // count of messages to drop
fn get_preview_elem_time/size/bytes(storage, idx) -> ...
fn truncate_doit(storage, time, side)  // perform actual truncate
fn clear_preview(storage)
```

---

## Phase C: Rust Time-Window Logic (NOT STARTED)

**Goal**: Move `MidiRingbufferCore` time-window operations to Rust.

### Tasks
- [ ] **C.1** Create `MidiTimeWindow` Rust struct
- [ ] **C.2** Implement time-window logic (overflow, truncation, snapshot)
- [ ] **C.3** Expose via CXX bridge
- [ ] **C.4** Update C++ `MidiRingbufferCore` to delegate
- [ ] **C.5** Test all time-window operations

### Implementation Notes
- Similar pattern to Phase B: use preview-then-act for any callbacks
- `next_buffer()` will need to truncate, following the same pattern
- `put()` delegates to `append()` on underlying storage
- `snapshot()` combines copy + truncate + time offset adjustment

### Verification
```bash
./target/debug/test_runner "[MidiRingbuffer]"  # All pass
./target/debug/test_runner "[chain]"           # All pass
```

---

## Phase D: Eliminate C++ MidiStorage (NOT STARTED)

**Goal**: Remove C++ `MidiStorage` class, use `RustMidiStorage` everywhere.

### Tasks
- [ ] **D.1** Make `MidiStorage` an alias for `RustMidiStorage`
- [ ] **D.2** Update `MidiRingbuffer` to use Rust storage directly
- [ ] **D.3** Remove/retain C++ storage files as needed
- [ ] **D.4** Update all consumers
- [ ] **D.5** Final integration test

### Verification
```bash
./target/debug/test_runner  # All 149 tests pass
```

---

## Phase E: Polish and Documentation (NOT STARTED)

### Tasks
- [ ] **E.1** Remove unused code and `#ifdef` guards
- [ ] **E.2** Update FFI documentation (now using return values + preview pattern)
- [ ] **E.3** Verify no memory leaks (valgrind/asan)
- [ ] **E.4** Performance comparison if benchmarks exist
- [ ] **E.5** Final documentation pass

### Verification
```bash
cargo build --release
./target/release/test_runner  # All tests pass
```

---

## Architecture (Current)

```
┌──────────────────────────────────────────────────────────────┐
│                     C++ Consumers                            │
│   MidiChannel, MidiPort, MidiRingbuffer                     │
└──────────────────────────────┬───────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│               IMidiStorage Interface (C++)                   │
│   Pure virtual - no implementation                           │
└──────────────────────────────┬───────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│               RustMidiStorage (C++ wrapper)                  │
│   Delegates to Rust via CXX FFI                               │
│   + m_rust_core (rust::Box<MidiStorageCore>)                 │
│   + sync_rust_state() for C++ data access                   │
└──────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌──────────────────────────────────────────────────────────────┐
│               MidiStorageCore (Rust)                         │
│   Full ringbuffer implementation                              │
│   + MidiCursor for iteration                                  │
│   + Preview buffers for truncate                             │
└──────────────────────────────────────────────────────────────┘
                             │
                    (Phase C will add)
                             ▼
┌──────────────────────────────────────────────────────────────┐
│               MidiTimeWindow (Rust) - FUTURE                │
│   Time-window logic                                          │
└──────────────────────────────────────────────────────────────┘
```

---

## Key Design Decisions (Completed)

### Callback Elimination
Instead of passing `std::function` callbacks across FFI, we use:
- **Return values** for single results (e.g., `append` returns flags)
- **Preview-then-act** pattern for multiple results (e.g., `truncate_preview` + `truncate_doit`)

This keeps the FFI boundary clean and Rust-idiomatic.

### FFI Pattern
All Rust functions exposed via CXX use:
- Simple types (u32, u16, bool)
- Raw pointers for buffer output (`*mut u8`, `*const u8`)
- Return values for status/size information

---

## Notes

### Memory Management
- CXX handles memory ownership automatically
- `rust::Box<T>` for Rust-owned objects passed to C++
- `shared_from_this()` pattern for cursor creation

### Testing Strategy
- Each phase must leave all tests passing
- 149 test cases, ~5894 assertions
- Run full suite before claiming phase complete

---

## Next Steps

1. **Start Phase C**: Create `MidiTimeWindow` Rust struct
2. Implement `set_n_samples()`, `get_n_samples()`, `next_buffer()`, `put()`, `snapshot()`
3. Use same preview-then-act pattern for any dropped messages
4. Test with `[MidiRingbuffer]` and `[chain]` test suites
5. Continue to Phase D when C is complete