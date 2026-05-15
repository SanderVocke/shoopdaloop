# Test Failure Analysis: MidiRingbuffer - Put then overflow then snapshot

## Test Location
`src/backend/test/unit/test_MidiRingbuffer.cpp:255-299`

## Current Test Status
**148/149 tests pass. Only this test fails.**

## Test Sequence
1. Create MidiRingbuffer with capacity 3
2. `set_n_samples(17)`
3. Loop `next_buffer(512)` until `current_buffer_end_time` approaches `u32::MAX - 2`
4. Put 3 messages at frames 0, 2, 5 (these get shifted into range during overflow)
5. Call `next_buffer(10)` - no overflow in this call
6. Call `snapshot(copy, 8)`

## Expected vs Actual (after step 5)
- **Expected `b`:** messages with timestamps 10, 12, 15
- **Actual `b`:** messages with timestamps 0, 2, 5

## Key Observation
The test expects `next_buffer(10)` to shift message timestamps by +10.
But based on the algorithm:
- `next_buffer` shifts messages ONLY during overflow
- Step 5 `next_buffer(10)` does NOT cause overflow (27 > 17)
- No shifting occurs, messages remain at their pre-overflow positions

## Actual Message Flow
1. Before overflow loop: messages at absolute times like 1000000000, etc.
2. During overflow handling: messages shifted to times like 7, 9, 12 (within window)
3. After step 4 put(): messages at times 7, 9, 12 (shifted during overflow)
4. Step 5 `next_buffer(10)`: window shifts to [17, 27), no overflow
   - Messages at times 7, 9 are below window, get truncated
   - Message at time 12 is kept
5. Result: only 1 message remains (at time 12), not 3 messages at 10, 12, 15

## Debug Output (current)
```
[DEBUG] MidiRingbufferCore::snapshot called
[DEBUG]   n_samples=17
[DEBUG]   current_buffer_end_time=17
[DEBUG]   current_buffer_start_time=7
[DEBUG]   after copy, target.n_events()=3
[DEBUG]   target.raw_tail()=0, target.raw_head()=0
[DEBUG]   end=17, start_from_end=8, min_message_time=9
[DEBUG]   after truncate, target.n_events()=0
```

## Root Cause Analysis
The test expectation appears to be incorrect, OR the C++ implementation
behaves differently than the documented algorithm. **Important: The test passes
on master (pure C++) but fails on midi (Rust implementation).**

The truncation condition `time >= min_time` keeps messages where `time >= 10`.
Messages at times 7 and 9 are truncated. Only message at time 12 is kept.

## Architecture Comparison

### Master Branch (Pure C++)
```
MidiRingbuffer (C++) → inherits from MidiStorage (C++)
  └── Uses native C++ MidiStorage for all operations
      └── for_each_msg_modify iterates through m_data directly
      └── truncate iterates through m_data directly
      └── snapshot: copy → truncate → for_each_msg_modify (timestamp adjust)
```

### Midi Branch (Rust Implementation)
```
MidiRingbuffer (C++) → MidiRingbufferCore (C++)
  └── MidiRingbufferCore uses m_storage: RustMidiStorage
        ├── C++ wrapper: RustMidiStorage.cpp
        └── Rust core: src/rust/backend_rust/src/midi_storage.rs
              ├── MidiTimeWindow (time window logic)
              └── MidiStorageCore (storage with ringbuffer)
        └── FFI bridge: src/rust/backend_rust/src/midi_storage_cxx.rs
```

## Key File Locations

### Master Branch (Pure C++)
- `src/backend/internal/MidiRingbuffer.cpp` - Original C++ implementation
- `src/backend/internal/MidiRingbuffer.h` - Header with class definition
- `src/backend/internal/MidiStorage.cpp` - Base storage implementation
- `src/backend/internal/MidiStorage.h` - Storage interface

### Midi Branch (Rust Integration)
- `src/backend/internal/MidiStorage.h` - Contains MidiRingbufferCore and MidiRingbuffer
- `src/backend/internal/MidiStorage.cpp` - MidiRingbufferCore implementation
- `src/backend/internal/RustMidiStorage.cpp` - C++ wrapper delegating to Rust
- `src/backend/internal/RustMidiStorage.h` - Header for RustMidiStorage
- `src/rust/backend_rust/src/midi_storage.rs` - Rust MidiTimeWindow and MidiStorageCore
- `src/rust/backend_rust/src/midi_storage_cxx.rs` - CXX FFI bridge

## Algorithm: next_buffer (C++ Original)
```cpp
void MidiRingbuffer::next_buffer(uint32_t n_frames, DroppedMsgCallback dropped_msg_cb) {
    uint32_t old_end = current_buffer_end_time.load();
    uint32_t new_end = old_end + n_frames;  // Note: may overflow

    if (new_end < old_end) {
        // Overflow occurring. Shift timestamps of whole buffer.
        const uint32_t moved_new_end = n_samples.load();
        const int shift = (int)moved_new_end - (int)new_end;
        Storage::for_each_msg_modify([shift](uint32_t &t, uint16_t &s, uint8_t *d) {
            t += shift;
        });
        new_end += shift;
        old_end += shift;
    }

    Storage::truncate(new_end - std::min(n_samples.load(), new_end),
                      Storage::TruncateSide::TruncateTail, dropped_msg_cb);
    current_buffer_start_time = old_end;
    current_buffer_end_time = new_end;
}
```

**Key behavior:** Messages are ONLY shifted during overflow (when `new_end < old_end`).

## Algorithm: snapshot (C++ Original)
```cpp
void MidiRingbuffer::snapshot(MidiStorage &target, std::optional<uint32_t> start_offset_from_end) const {
    Storage::copy(target);
    auto const end = current_buffer_end_time.load();
    auto const start_from_end = start_offset_from_end.value_or(n_samples.load());
    const uint32_t min_message_time = end - std::min(end, start_from_end);
    target.truncate(min_message_time, Storage::TruncateSide::TruncateTail);
    target.for_each_msg_modify([min_message_time](uint32_t &t, uint16_t &s, uint8_t *d) {
        t -= min_message_time;
    });
}
```

## Current State on Midi Branch

### Rust MidiTimeWindow::next_buffer
- Uses `wrapping_add` for overflow detection
- Captures `n_samples` before potential modification
- Shifts messages and boundaries during overflow only
- Truncates with `saturating_sub` for min_time
- Delegates truncate to Rust `MidiStorageCore::truncate`

### Rust MidiTimeWindow::snapshot
- Copies source to target via `storage.copy_to(target)`
- Calculates `min_message_time = end.wrapping_sub(min(end, start_from_end))`
- Truncates target via `target.truncate_no_callback(min_message_time, TruncateTail)`
- Adjusts timestamps via `target.for_each_msg_modify(|t, _, _| t.wrapping_sub(min_message_time))`

### CXX FFI Bridge
The `time_window_snapshot` function in `midi_storage_cxx.rs` swaps parameter order:
```rust
// storage (ringbuffer source) goes to target parameter
// target (snapshot) goes to storage parameter
window.snapshot(storage, target, start_offset_from_end);
```

### RustMidiStorage::snapshot
Calls the FFI bridge and syncs state:
```cpp
void RustMidiStorage::snapshot(RustMidiStorage& target, std::optional<uint32_t> start_offset_from_end) const {
    uint32_t offset = start_offset_from_end.value_or(get_n_samples());
    backend_rust::time_window_snapshot(*m_time_window, ..., *target.m_rust_core, offset);
    target.sync_rust_state();
}
```

## Truncation Logic

### C++ TruncateTail (MidiStorage.cpp)
```cpp
if (side == MidiStorageTruncateSide::TruncateTail) {
    auto &e = m_data[m_tail];
    if (!should_truncate_fn(e.time, e.size, e.data())) return;  // Early exit if tail is valid

    uint32_t idx = m_tail;
    uint32_t dropped = 0;
    for (uint32_t i = 0; i < m_n_events; ++i) {
        auto &elem = m_data[idx];
        if (!should_truncate_fn(elem.time, elem.size, elem.data())) {
            break;  // Stop when we hit a valid message
        }
        dropped++;
        idx = (idx + 1) % cap;
    }
    m_tail = idx;
    m_n_events -= dropped;
}
```

### Rust truncate (midi_storage.rs)
```rust
TruncateSide::TruncateTail => {
    if let Some(elem) = self.data.get(self.tail as usize) {
        if elem.time >= time {
            return;  // Early exit if tail is valid
        }
    }

    let mut idx = self.tail;
    let mut dropped = 0u32;
    for _ in 0..self.n_events {
        if let Some(elem) = self.data.get(idx as usize) {
            if elem.time >= time {
                break;  // Stop when we hit a valid message
            }
        }
        dropped += 1;
        idx = (idx + 1) % capacity;
    }

    self.tail = idx;
    self.n_events -= dropped;
    if self.n_events == 0 {
        self.head = self.tail;
    }
}
```

## Commit History (Relevant to this Issue)
- `ad563f61` - "Fix Rust MidiTimeWindow overflow and snapshot parameter ordering"
- `1878e781` - "Fix MidiTimeWindow timestamp handling to match C++ behavior"
- `70f657af` - "back to callbacks"
- `04205789` - "Phase A: Implement Rust MidiCursor and expose via CXX bridge"
- `bada6aa0` - "Phase 5-6: Add Rust MIDI storage with CXX bridge and hybrid wrapper"

## Build Instructions
```bash
# Full rebuild
cargo build

# Run just the failing test
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner "MidiRingbuffer - Put then overflow then snapshot"

# Run all tests
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner
```

## Test Instructions
```bash
# Run all tests and show summary
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner 2>&1 | grep -E "test cases:|assertions:"

# Run failing test only
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner "MidiRingbuffer - Put then overflow then snapshot"

# Show debug output
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner "MidiRingbuffer - Put then overflow then snapshot" 2>&1 | grep "DEBUG\|RUST\|C\+\+"
```

## Remaining Question
If the Rust implementation matches the C++ algorithm, why does the test pass on master but fail on midi? Possible causes:
1. The Rust implementation has a subtle bug that the C++ doesn't have
2. The FFI bridge introduces incorrect behavior
3. The state synchronization between Rust and C++ is incomplete
4. The test expectation itself is incorrect (worked by accident on master due to different internal timing)