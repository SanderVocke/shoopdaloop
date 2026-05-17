# MIDI Port Rust Porting

Port the C++ MIDI port classes to Rust in the `backend_rust` crate, using CXX bridges for C++ interop.

**Build/Test Reference:** See `build_instructions.md`:
- Full build: `cargo build` (includes Rust + C++)
- Unit tests: `target/debug/build/backend-*/out/cmake_build/build/test/test_runner`
- Integration tests: `./target/debug/shoopdaloop_dev.sh --self-test`

---

## Prerequisites

Before starting, ensure the project builds and tests pass:

- [ ] Run `cargo build` - must succeed
- [ ] Run `test_runner` - must pass (5894 assertions in 149 test cases)
- [ ] Run `shoopdaloop_dev.sh --self-test` - must pass (186 test cases)

---

## Step 1: Define Rust Traits ✅

Create trait files corresponding to C++ interfaces.

### 1.1 Create `backend_rust/src/midi_traits.rs` ✅

```rust
// MIDI trait definitions corresponding to C++ interfaces:
// - IMidiStateTracking -> trait MidiStateTracking
// - IMidiRingbuffer -> trait MidiRingbufferOps
// - IMidiReadableBuffer -> trait MidiReadableBuffer
// - IMidiWriteableBuffer -> trait MidiWritableBuffer

use crate::midi_storage::RustMidiStorage;

/// Corresponds to C++ IMidiStateTracking
pub trait MidiStateTracking {
    fn n_notes_active(&self) -> u32;
    fn input_event_count(&self) -> u32;
    fn output_event_count(&self) -> u32;
}

/// Corresponds to C++ IMidiRingbuffer
pub trait MidiRingbufferOps {
    fn set_n_samples(&mut self, n: u32);
    fn get_n_samples(&self) -> u32;
    fn snapshot(&self, target: &mut RustMidiStorage, start_offset: Option<u32>);
    fn get_current_n_samples(&self) -> u32 { self.get_n_samples() }
}

/// Corresponds to C++ IMidiReadableBuffer
pub trait MidiReadableBuffer {
    fn n_events(&self) -> u32;
    fn get_event(&self, idx: u32) -> MidiStorageElem;
}

/// Corresponds to C++ IMidiWriteableBuffer
pub trait MidiWritableBuffer {
    fn write_event(&mut self, event: MidiStorageElem);
}

pub struct MidiStorageElem { /* ... */ }
```

- [x] Create `backend_rust/src/midi_traits.rs` with traits above
- [x] **Build:** `cargo build` - must succeed ✅
- [x] Add unit tests for trait implementations ✅

---

## Step 2: Port MidiPortBase ✅

Port the core logic holder that holds state tracking, ringbuffer, and event counters.

### 2.1 Create `backend_rust/src/midi_port_base.rs` ✅

Corresponds to C++ `MidiPortBase.h/cpp`. Key fields from C++:
- `m_maybe_midi_state: shoop_shared_ptr<MidiStateTracker>`
- `m_ringbuffer_tail_state: shoop_shared_ptr<MidiStateTracker>`
- `m_midi_ringbuffer: shoop_shared_ptr<MidiRingbuffer>`
- `n_input_events`, `n_output_events: std::atomic<uint32_t>`
- `m_muted: std::atomic<bool>`

Implement:
- `MidiStateTracking` trait
- `MidiRingbufferOps` trait
- Methods: `increment_input_events()`, `increment_output_events()`, `process_msg_to_state()`

Use `Arc<AtomicU32>` for atomic counters and `Arc<Mutex<T>>` or `parking_lot::Mutex<T>` for shared state.

- [x] Create `backend_rust/src/midi_port_base.rs`
- [x] Implement struct with all fields from C++ MidiPortBase
- [x] Implement `MidiStateTracking` trait
- [x] Implement `MidiRingbufferOps` trait
- [x] Add internal helper methods

### 2.2 Create `backend_rust/src/midi_port_base_cxx.rs` ✅

Create CXX bridge to expose `MidiPortBase` to C++.

```rust
#[cxx::bridge]
mod ffi {
    extern "Rust" {
        type MidiPortBase;
        
        // MidiStateTracking
        fn n_notes_active(self: &MidiPortBase) -> u32;
        fn input_event_count(self: &MidiPortBase) -> u32;
        fn output_event_count(self: &MidiPortBase) -> u32;
        
        // MidiRingbufferOps
        fn set_n_samples(self: &mut MidiPortBase, n: u32);
        fn get_n_samples(self: &MidiPortBase) -> u32;
        fn snapshot(self: &MidiPortBase, target: &mut RustMidiStorage, start_offset: Option<u32>);
        
        // Internal helpers
        fn increment_input_events(self: &MidiPortBase, count: u32);
        fn increment_output_events(self: &MidiPortBase, count: u32);
        fn reset_n_input_events(self: &MidiPortBase);
        fn reset_n_output_events(self: &MidiPortBase);
    }
}
```

- [x] Create `backend_rust/src/midi_port_base_cxx.rs` with CXX bridge
- [x] Update `backend_rust/src/lib.rs` to include new modules
- [x] **Build:** `cargo build` - must succeed ✅
- [x] Verify CXX header generation ✅

---

## Step 3: Port MidiPort ✅

Port the main MIDI port class that contains MidiPortBase and adds buffer management.

### 3.1 Create `backend_rust/src/midi_port.rs` ✅

Corresponds to C++ `MidiPort.h/cpp`. Key from C++:
- Contains `MidiPortBase m_base`
- Buffer pointers (can use `AtomicPtr` or optional trait objects)
- Delegates to `m_base` for state tracking and ringbuffer

Implement:
- State tracking (delegate to base)
- Ringbuffer methods (delegate to base)
- Mute state (delegate to base)
- Buffer interface accessors (get_readable_buffer, etc.)

- [x] Create `backend_rust/src/midi_port.rs` ✅
- [x] Implement struct with `MidiPortBase` field
- [x] Delegate all state tracking methods to base
- [x] Delegate all ringbuffer methods to base
- [x] Implement mute methods (set_muted, get_muted)

### 3.2 Create `backend_rust/src/midi_port_cxx.rs` ✅

Create CXX bridge for MidiPort.

- [x] Create `backend_rust/src/midi_port_cxx.rs` with CXX bridge
- [x] Expose constructor and all public methods
- [x] **Build:** `cargo build` - must succeed ✅

---

## Step 4: Update C++ MidiPort

Update C++ to delegate to Rust via CXX bridge.

### 4.1 Update `backend/internal/MidiPort.h`

Add forward declaration to Rust type and change implementation to delegate:

```cpp
#pragma once
#include "PortInterface.h"
#include "IMidiStateTracking.h"
#include "IMidiRingbuffer.h"
#include "midi_port_cxx.h"  // CXX bridge header

class MidiPort : public virtual PortInterface,
                 public virtual IMidiStateTracking,
                 public virtual IMidiRingbuffer {
    // Bridge to Rust implementation
    std::unique_ptr<medi::MidiPort> m_rust_port;
    // ... buffer pointers if still needed in C++
    
public:
    MidiPort(bool track_notes, bool track_controls, bool track_programs);
    virtual ~MidiPort();
    
    // Delegate all methods to m_rust_port
    uint32_t n_notes_active() const override { 
        return m_rust_port->n_notes_active(); 
    }
    // ... etc
};
```

- [ ] Update `backend/internal/MidiPort.h` to use Rust bridge
- [ ] Remove MidiPortBase composition (now in Rust)

### 4.2 Update `backend/internal/MidiPort.cpp`

Implement methods by delegating to Rust:

```cpp
#include "MidiPort.h"
#include "midi_port_cxx.h"

MidiPort::MidiPort(bool track_notes, bool track_controls, bool track_programs)
    : m_rust_port(media::new_midi_port(track_notes, track_controls, track_programs))
{}

MidiPort::~MidiPort() = default;

uint32_t MidiPort::n_notes_active() const {
    return m_rust_port->n_notes_active();
}

// ... delegate all other methods
```

- [ ] Update `backend/internal/MidiPort.cpp` to delegate to Rust
- [ ] **Build:** `cargo build` - must succeed
- [ ] **Tests:** `test_runner` - must pass

---

## Step 5: Port DummyMidiPort ✅

Port the test-specific implementation.

### 5.1 Create `backend_rust/src/dummy_midi_port.rs` ✅

Corresponds to C++ `DummyMidiPort.h/cpp`. Inherits from MidiPort (Rust) and implements buffer interfaces.

- [x] Create `backend_rust/src/dummy_midi_port.rs` ✅
- [x] Implement buffer traits (MidiReadableBuffer, MidiWritableBuffer) ✅
- [x] Implement queue/message management ✅

### 5.2 Create `backend_rust/src/dummy_midi_port_cxx.rs` ✅

- [x] Create CXX bridge for DummyMidiPort ✅
- [x] **Build:** `cargo build` - must succeed ✅

### 5.3 Update C++ DummyMidiPort

- [ ] Update `backend/internal/DummyMidiPort.h` to delegate to Rust
- [ ] Update `backend/internal/DummyMidiPort.cpp` to delegate to Rust
- [ ] **Build:** `cargo build` - must succeed
- [ ] **Tests:** `test_runner` - must pass
- [ ] **Integration:** `shoopdaloop_dev.sh --self-test` - must pass

---

## Step 6: Port Remaining Classes (Optional)

These classes may be optional depending on architecture decisions:

### 6.1 MidiBufferingInputPort
- [ ] Create `backend_rust/src/midi_buffering_input_port.rs`
- [ ] Update C++ to delegate if needed

### 6.2 GraphMidiPort  
- [ ] Review - may not need Rust port if C++ delegation is sufficient

---

## Verification Checklist

After each step, verify:

- [ ] **Build:** `cargo build` succeeds
- [ ] **Unit Tests:** `test_runner` passes
- [ ] **Integration:** `shoopdaloop_dev.sh --self-test` passes

---

## Architecture Notes

### Thread Safety
- Use `AtomicU32`/`AtomicU64` for counters
- Use `Mutex<T>` or `RwLock<T>` for complex shared state
- Use `Arc<T>` for shared ownership

### CXX Bridge Patterns
```rust
// Constructor returns Box
fn new_midi_port(...) -> Box<MidiPort>

// Methods use references
fn process(port: &mut MidiPort, n_frames: u32);
fn n_notes_active(port: &MidiPort) -> u32;
```

### Existing Backend Rust Crate
The `backend_rust` crate already contains related implementations:
- `midi_storage.rs` - MIDI storage (RustMidiStorage)
- `midi_state_tracker.rs` - MIDI state tracking  
- `midi_ringbuffer.rs` - Ringbuffer (RustMidiStorage)

Reuse these where applicable.

---

## File Map

| C++ File | Rust File |
|----------|-----------|
| `MidiPortBase.h/cpp` | `midi_port_base.rs`, `midi_port_base_cxx.rs` |
| `MidiPort.h/cpp` | `midi_port.rs`, `midi_port_cxx.rs` |
| `DummyMidiPort.h/cpp` | `dummy_midi_port.rs`, `dummy_midi_port_cxx.rs` |
| `IMidiStateTracking.h` | `midi_traits.rs` (trait MidiStateTracking) |
| `IMidiRingbuffer.h` | `midi_traits.rs` (trait MidiRingbufferOps) |
| `IMidiReadableBuffer.h` | `midi_traits.rs` (trait MidiReadableBuffer) |
| `IMidiWriteableBuffer.h` | `midi_traits.rs` (trait MidiWritableBuffer) |