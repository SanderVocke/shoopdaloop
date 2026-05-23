# Plan: Port BasicLoop and AudioMidiLoop to Rust

## Overview

Port `BasicLoop` and `AudioMidiLoop` classes from C++ to Rust, keeping them accessible via CXX bridges. BasicLoop implements core loop semantics; AudioMidiLoop extends BasicLoop with audio/midi channels. The channels themselves remain in C++ for now, accessed via opaque pointers and FFI callbacks.

## Code Locations

### BasicLoop
- **C++ Source**: `src/backend/internal/BasicLoop.h`, `src/backend/internal/BasicLoop.cpp`
- **C++ Interface**: `src/backend/internal/LoopInterface.h`
- **Rust Implementation**: `src/rust/backend_rust/src/basic_loop.rs` (DONE)
- **Rust CXX Bridge**: `src/rust/backend_rust/src/basic_loop_cxx.rs` (DONE)

### AudioMidiLoop
- **C++ Source**: `src/backend/internal/AudioMidiLoop.h`, `src/backend/internal/AudioMidiLoop.cpp`
- **Rust Destination**: `src/rust/backend_rust/src/audio_midi_loop.rs`
- **Rust CXX Bridge**: `src/rust/backend_rust/src/audio_midi_loop_cxx.rs`

### Channels (remain in C++, accessed via FFI)
- **AudioChannel**: `src/backend/internal/AudioChannel.h`, `src/backend/internal/AudioChannel.cpp`
- **MidiChannel**: `src/backend/internal/MidiChannel.h`, `src/backend/internal/MidiChannel.cpp`
- **ChannelInterface**: `src/backend/internal/ChannelInterface.h`

### Tests
- **BasicLoop**: `src/backend/test/unit/test_BasicLoop.cpp`
- **AudioMidiLoop Audio**: `src/backend/test/unit/test_AudioMidiLoop_audio.cpp`
- **AudioMidiLoop Midi**: `src/backend/test/unit/test_AudioMidiLoop_midi.cpp`
- **Synced Loops**: `src/backend/test/integration/test_synced_BasicLoops.cpp`

### Dependencies Already Ported
- `CommandQueue` - `src/rust/backend_rust/src/command_queue.rs`
- `MidiStorage` - `src/rust/backend_rust/src/midi_storage.rs`
- `MidiStateTracker` - `src/rust/backend_rust/src/midi_state_tracker.rs`

## Architecture

### Inheritance Structure
```
LoopInterface (C++ abstract interface)
    └── BasicLoop (Rust + C++ wrapper)
            └── AudioMidiLoop (Rust + C++ wrapper)
                    └── Wrapped by GraphLoop (C++)
```

### AudioMidiLoop State
AudioMidiLoop extends BasicLoop with channel collections:

```
AudioMidiLoop : BasicLoop
├── mp_audio_channels (Vec<usize> - opaque pointers to C++ AudioChannel)
├── mp_midi_channels (Vec<usize> - opaque pointers to C++ MidiChannel)
├── Overrides:
│   ├── PROC_update_poi() - merges channel POIs
│   ├── PROC_handle_poi() - calls channel POI handlers
│   ├── PROC_process_channels() - processes all channels
```

### Key Methods to Port for AudioMidiLoop

1. **Channel Management** (thread-safe via callback to C++):
   - `add_audio_channel()` - creates channel in C++, stores pointer
   - `add_midi_channel()` - creates channel in C++, stores pointer
   - `audio_channel(idx)` - retrieves pointer, casts in C++
   - `midi_channel(idx)` - retrieves pointer, casts in C++
   - `delete_audio_channel()` - removes pointer, C++ deletes object
   - `delete_midi_channel()` - removes pointer, C++ deletes object
   - `n_audio_channels()`, `n_midi_channels()` - count getters

2. **Process-thread overrides**:
   - `PROC_update_poi()` - calls channels' PROC_get_next_poi via FFI, merges results
   - `PROC_handle_poi()` - calls channels' PROC_handle_poi via FFI
   - `PROC_process_channels()` - calls channels' PROC_process via FFI

### FFI Interface for Channel Operations

Rust needs C++ callbacks for channel operations since channels remain in C++:

```rust
// FFI functions provided by C++ for channel operations
extern "C" {
    fn channel_proc_get_next_poi(
        channel_ptr: usize,
        mode: u32,
        next_mode: u32, // 0xFF for none
        next_delay: i32, // -1 for none
        next_eta: u32, // 0xFF for none
        length: u32,
        position: u32
    ) -> i32; // -1 for none, else samples
    
    fn channel_proc_handle_poi(
        channel_ptr: usize,
        mode: u32,
        length: u32,
        position: u32
    );
    
    fn channel_proc_process(
        channel_ptr: usize,
        mode: u32,
        next_mode: u32,
        next_delay: i32,
        next_eta: u32,
        n_samples: u32,
        pos_before: u32,
        pos_after: u32,
        length_before: u32,
        length_after: u32
    );
}
```

## Implementation Phases

### Phase A: BasicLoop C++ Wrapper Integration (Completing Phase 9)

Now that AudioMidiLoop will be ported, the BasicLoop C++ wrapper can proceed:
- Modify BasicLoop.h/cpp to delegate to Rust BasicLoopCore
- AudioMidiLoop will inherit from the C++ wrapper but use its own Rust core

### Phase B: AudioMidiLoop Rust Implementation

Create `audio_midi_loop.rs`:
- `AudioMidiLoopCore` struct extending BasicLoopCore concept
- Channel vectors as `Vec<usize>` (opaque pointers)
- All methods that call channels use FFI callbacks

Key design: AudioMidiLoopCore does NOT inherit from BasicLoopCore in Rust. Instead:
- AudioMidiLoopCore contains a BasicLoopCore instance
- Or: AudioMidiLoopCore has all BasicLoopCore fields + additional channel fields
- C++ wrapper handles inheritance relationship

### Phase C: AudioMidiLoop CXX Bridge

Create `audio_midi_loop_cxx.rs`:
- Expose `AudioMidiLoopCore` type
- Channel management functions (return pointers, C++ creates objects)
- Process thread functions (FFI callbacks to channels)

### Phase D: Channel FFI Helpers

Create C++ FFI helper functions for channel operations:
- Add to existing C++ file or new file
- Functions callable from Rust via extern "C"
- Handle casting from usize to actual channel types

### Phase E: AudioMidiLoop C++ Wrapper

Modify AudioMidiLoop.h/cpp:
- Replace internal state with Rust AudioMidiLoopCore
- Channel creation returns pointers stored in Rust
- Methods delegate to Rust
- FFI helpers registered with Rust core

### Phase F: Test Verification

- Run all Rust tests
- Run all C++ tests (BasicLoop, AudioMidiLoop)
- Run integration tests

## Build and Test Instructions

### Build
```bash
cargo build
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
```

### Test
```bash
# Rust unit tests
cargo test basic_loop
cargo test audio_midi_loop

# C++ tests
cargo build
./target/debug/test_runner "[BasicLoop]"
./target/debug/test_runner "[AudioMidiLoop]"
./target/debug/test_runner "[SyncedBasicLoops]"
```

## Key Challenges and Solutions

### Challenge 1: Inheritance Across FFI

C++ AudioMidiLoop inherits from BasicLoop. Rust doesn't have class inheritance.

**Solution**: Each Rust core (BasicLoopCore, AudioMidiLoopCore) is independent. C++ wrappers maintain inheritance. AudioMidiLoopCore contains all BasicLoopCore fields plus channel fields.

### Challenge 2: Channels Remain in C++

AudioChannel and MidiChannel are complex, remain in C++ for now.

**Solution**: Store channels as opaque `usize` pointers in Rust. Provide C++ FFI callbacks for channel operations that Rust can call.

### Challenge 3: PROC_update_poi Needs BasicLoop State

AudioMidiLoop::PROC_update_poi needs mp_next_poi, mp_next_trigger, planned mode/delay from BasicLoop.

**Solution**: AudioMidiLoopCore contains all these fields. PROC_update_poi in AudioMidiLoopCore can directly access them.

### Challenge 4: GraphLoop Uses AudioMidiLoop

GraphLoop wraps AudioMidiLoop with graph connectivity.

**Solution**: GraphLoop remains in C++. It uses AudioMidiLoop C++ wrapper (which delegates to Rust). GraphLoop's reference to AudioMidiLoop loop member remains unchanged.

## Success Criteria

1. All existing C++ tests pass unchanged
2. Rust unit tests cover same scenarios
3. No compiler warnings with `RUSTFLAGS="-D warnings"`
4. Code formatted with `cargo fmt`
5. BasicLoop and AudioMidiLoop fully functional via Rust cores
6. Channels continue to work correctly via FFI callbacks