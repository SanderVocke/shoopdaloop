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
The test expectation appears to be incorrect, or the C++ implementation
behaves differently than the documented algorithm. The Rust implementation
appears to correctly match the C++ MidiRingbufferCore algorithm.

The truncation condition `time >= min_time` keeps messages where `time >= 10`.
Messages at times 7 and 9 are truncated. Only message at time 12 is kept.

## Files Modified (debugging)
- `src/rust/backend_rust/src/midi_storage.rs` - Added eprintln! debug
- `src/rust/backend_rust/src/midi_storage_cxx.rs` - Added eprintln! debug
- `src/backend/internal/RustMidiStorage.cpp` - Added std::cerr debug

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