# MIDI Port Rust Porting Guide

This document outlines the plan for porting C++ MIDI port classes to Rust, making them compatible with the existing `backend_rust` crate structure.

## Overview

The `backend_rust` crate already contains several MIDI-related Rust implementations exposed to C++ via CXX bridges:
- `midi_storage` / `midi_storage_cxx` - MIDI storage (RustMidiStorage)
- `midi_state_tracker` / `midi_state_tracker_cxx` - MIDI state tracking
- `midi_sorting_buffer` / `midi_sorting_buffer_cxx` - MIDI sorting buffer

The goal is to port the remaining C++ MIDI port classes to Rust, following the same pattern.

## Architecture

### Current C++ Structure

```
MidiPortBase (MidiPortBase.h/cpp)
├── IMidiStateTracking (state: n_notes_active, event counts)
├── IMidiRingbuffer (ringbuffer ops: set_n_samples, snapshot)
├── MidiStateTracker (m_maybe_midi_state, m_ringbuffer_tail_state)
├── MidiRingbuffer (m_midi_ringbuffer)
├── Atomic counters (n_input_events, n_output_events)
└── Mute state (m_muted)

MidiPort (MidiPort.h/cpp) - extends MidiPortBase
├── Buffer pointers (ma_write_data_into_port_buffer, etc.)
├── PortInterface methods
└── Buffer accessors

DummyMidiPort - extends MidiPort
├── WithCommandQueue
├── MidiReadableBuffer, MidiWriteableBuffer
└── Queue/message management

MidiBufferingInputPort - extends MidiPort
├── MidiReadableBuffer
└── Internal buffer storage

GraphMidiPort - contains MidiPort via shoop_shared_ptr
```

### Target Rust Structure

```
backend_rust/src/
├── midi_port_base.rs      # MidiPortBase equivalent
├── midi_port_base_cxx.rs  # CXX bridge for MidiPortBase
├── midi_port.rs           # MidiPort equivalent  
├── midi_port_cxx.rs       # CXX bridge for MidiPort
├── midi_readable_buffer.rs   # Trait + implementations
├── midi_writeable_buffer.rs   # Trait + implementations
├── dummy_midi_port.rs         # DummyMidiPort equivalent
└── dummy_midi_port_cxx.rs     # CXX bridge

# Reuse existing:
├── midi_storage.rs           # Already in backend_rust
├── midi_state_tracker.rs      # Already in backend_rust
├── midi_ringbuffer.rs         # Already in backend_rust (RustMidiStorage)
```

## Porting Steps

### Phase 1: Core Traits and Buffers

First, port the small buffer classes and define traits that map to C++ interfaces.

#### 1.1 Define Rust Traits (Port Existing C++ Interfaces)

```rust
// src/midi_traits.rs
// Corresponds to C++: IMidiStateTracking, IMidiRingbuffer

pub trait MidiStateTracking {
    fn n_notes_active(&self) -> u32;
    fn input_event_count(&self) -> u32;
    fn output_event_count(&self) -> u32;
}

pub trait MidiRingbufferOps {
    fn set_n_samples(&mut self, n: u32);
    fn get_n_samples(&self) -> u32;
    fn get_current_n_samples(&self) -> u32;
    fn snapshot(&self, target: &mut RustMidiStorage, start_offset: Option<u32>);
}
```

#### 1.2 Buffer Traits

```rust
// src/midi_buffer_trait.rs
// Corresponds to C++: IMidiReadableBuffer, IMidiWriteableBuffer

use crate::midi_storage::MidiStorageElem;

pub trait MidiReadableBuffer {
    fn n_events(&self) -> u32;
    fn get_event(&self, idx: u32) -> MidiStorageElem;
}

pub trait MidiWritableBuffer {
    fn write_event(&mut self, event: MidiStorageElem);
}
```

### Phase 2: MidiPortBase

Port the core MIDI port logic holder.

#### 2.1 Create Rust Struct

```rust
// src/midi_port_base.rs

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::midi_state_tracker::MidiStateTracker;
use crate::midi_storage::RustMidiStorage;
use crate::midi_ringbuffer::MidiRingbuffer;

pub struct MidiPortBase {
    // Tracking configuration
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    
    // State trackers
    maybe_midi_state: Option<Arc<MidiStateTracker>>,
    ringbuffer_tail_state: Arc<MidiStateTracker>,
    
    // Ringbuffer
    midi_ringbuffer: Option<Arc<MidiRingbuffer>>,
    
    // Counters (use AtomicU32 for thread safety)
    input_events: AtomicU32,
    output_events: AtomicU32,
    
    // Mute state
    muted: AtomicBool, // or use a mutex for more complex state
}

impl MidiPortBase {
    pub fn new(track_notes: bool, track_controls: bool, track_programs: bool) -> Self { ... }
    
    // Delegate to state tracker
    pub fn n_notes_active(&self) -> u32 { ... }
    pub fn input_event_count(&self) -> u32 { self.input_events.load(Ordering::Relaxed) }
    pub fn output_event_count(&self) -> u32 { self.output_events.load(Ordering::Relaxed) }
    
    // Event management
    pub fn increment_input_events(&self, count: u32) { ... }
    pub fn increment_output_events(&self, count: u32) { ... }
    pub fn reset_n_input_events(&self) { ... }
    pub fn reset_n_output_events(&self) { ... }
    
    // State processing
    pub fn process_msg_to_state(&self, data: &[u8]) { ... }
    
    // Ringbuffer operations
    pub fn set_n_samples(&mut self, n: u32) { ... }
    pub fn get_n_samples(&self) -> u32 { ... }
    pub fn snapshot(&self, target: &mut RustMidiStorage, start_offset: Option<u32>) { ... }
}

impl MidiStateTracking for MidiPortBase { ... }
impl MidiRingbufferOps for MidiPortBase { ... }
```

#### 2.2 Create CXX Bridge

```rust
// src/midi_port_base_cxx.rs

use crate::midi_port_base::MidiPortBase;
use crate::midi_storage::RustMidiStorage;

#[cxx::bridge]
mod ffi {
    extern "Rust" {
        type MidiPortBase;
        
        fn n_notes_active(self: &MidiPortBase) -> u32;
        fn input_event_count(self: &MidiPortBase) -> u32;
        fn output_event_count(self: &MidiPortBase) -> u32;
        
        fn set_n_samples(self: &mut MidiPortBase, n: u32);
        fn get_n_samples(self: &MidiPortBase) -> u32;
        fn snapshot(self: &MidiPortBase, target: &mut RustMidiStorage, start_offset: Option<u32>);
    }
}
```

### Phase 3: MidiPort

Port the main MIDI port class that inherits from MidiPortBase.

#### 3.1 Create Rust Struct

```rust
// src/midi_port.rs

use std::sync::atomic::{AtomicPtr, Ordering};

use crate::midi_port_base::MidiPortBase;
use crate::midi_buffer_trait::{MidiReadableBuffer, MidiWritableBuffer};
use crate::midi_storage::MidiStorageElem;

pub struct MidiPort {
    base: MidiPortBase,
    
    // Buffer pointers (atomic for thread safety)
    write_data_into_port_buffer: AtomicPtr<()>,  // MidiWriteableBuffer*
    read_output_data_buffer: AtomicPtr<()>,      // MidiReadableBuffer*
    internal_read_input_data_buffer: AtomicPtr<()>,
    internal_write_output_data_to_buffer: AtomicPtr<()>,
    
    name: String,
    direction: PortDirection,
}

impl MidiPort {
    pub fn new(track_notes: bool, track_controls: bool, track_programs: bool) -> Self { ... }
    
    // Buffer accessors (virtual methods - use trait objects or enums)
    pub fn get_readable_buffer(&self) -> Option<&dyn MidiReadableBuffer> { None }
    pub fn get_writeable_buffer(&self) -> Option<&mut dyn MidiWritableBuffer> { None }
    
    // PortInterface-like methods (delegated to C++ via bridge)
    pub fn name(&self) -> &str { &self.name }
    pub fn direction(&self) -> PortDirection { self.direction }
    pub fn type_(&self) -> PortDataType { PortDataType::Midi }
    
    // State tracking (delegated to base)
    pub fn n_notes_active(&self) -> u32 { self.base.n_notes_active() }
    pub fn input_event_count(&self) -> u32 { self.base.input_event_count() }
    pub fn output_event_count(&self) -> u32 { self.base.output_event_count() }
    
    // Ringbuffer (delegated to base)
    pub fn set_ringbuffer_n_samples(&mut self, n: u32) { self.base.set_n_samples(n) }
    pub fn get_ringbuffer_n_samples(&self) -> u32 { self.base.get_n_samples() }
    
    // Process methods (callbacks from C++)
    pub fn prepare(&mut self, n_frames: u32) { ... }
    pub fn process(&mut self, n_frames: u32) { ... }
}

// Re-export traits for CXX bridge
pub use crate::midi_port_base::MidiPortBase as MidiPortBaseType;
```

#### 3.2 CXX Bridge

```rust
// src/midi_port_cxx.rs

#[cxx::bridge]
mod ffi {
    extern "Rust" {
        type MidiPort;
        
        fn new_midi_port(track_notes: bool, track_controls: bool, track_programs: bool) -> Box<MidiPort>;
        fn name(self: &MidiPort) -> String;
        fn n_notes_active(self: &MidiPort) -> u32;
        fn input_event_count(self: &MidiPort) -> u32;
        fn output_event_count(self: &MidiPort) -> u32;
        fn set_ringbuffer_n_samples(self: &mut MidiPort, n: u32);
        fn get_ringbuffer_n_samples(self: &MidiPort) -> u32;
        fn set_muted(self: &mut MidiPort, muted: bool);
        fn get_muted(self: &MidiPort) -> bool;
        
        // Process callbacks (called from C++)
        fn prepare(self: &mut MidiPort, n_frames: u32);
        fn process(self: &mut MidiPort, n_frames: u32);
    }
}
```

### Phase 4: DummyMidiPort

Port the test-specific implementation.

#### 4.1 Create Rust Struct

```rust
// src/dummy_midi_port.rs

use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use crate::midi_port::MidiPort;
use crate::midi_storage::MidiStorageElem;

pub struct DummyMidiPort {
    port: MidiPort,
    
    // Queue management
    queued_msgs: Arc<Mutex<VecDeque<MidiStorageElem>>>,
    written_requested_msgs: Arc<Mutex<Vec<MidiStorageElem>>>,
    buffer_data: Arc<Mutex<Vec<MidiStorageElem>>>,
    
    // Frame tracking
    current_buf_frames: u32,
    n_requested_frames: u32,
    n_original_requested_frames: u32,
}

impl DummyMidiPort {
    pub fn new(name: String, direction: PortDirection) -> Self { ... }
    
    // Buffer interface implementation
    pub fn n_events(&self) -> u32 { ... }
    pub fn get_event(&self, idx: u32) -> MidiStorageElem { ... }
    pub fn write_event(&mut self, event: MidiStorageElem) { ... }
    
    // Queue management
    pub fn queue_msg(&mut self, time: u32, data: &[u8]) { ... }
    pub fn request_data(&mut self, n_frames: u32) { ... }
    pub fn get_written_requested_msgs(&mut self) -> Vec<MidiStorageElem> { ... }
}

impl MidiReadableBuffer for DummyMidiPort { ... }
impl MidiWritableBuffer for DummyMidiPort { ... }
```

### Phase 5: Integration with C++

The C++ side will be updated to use the Rust implementations via CXX.

#### 5.1 Update CMakeLists.txt

Add `backend_rust` as a dependency in the C++ build system.

#### 5.2 Update C++ Headers

Replace C++ class definitions with forward declarations to Rust types:

```cpp
// MidiPort.h (updated)
#pragma once
#include "PortInterface.h"
#include "IMidiStateTracking.h"
#include "IMidiRingbuffer.h"
#include "midi_port_cxx.h"  // CXX bridge header

class MidiPort : public virtual PortInterface,
                 public virtual IMidiStateTracking,
                 public virtual IMidiRingbuffer {
    // Bridge to Rust implementation
    RustMidiPort* m_rust_port;  // or UniquePtr<RustMidiPort>
    
public:
    // Delegate all methods to m_rust_port
    uint32_t n_notes_active() const override { 
        return m_rust_port->n_notes_active(); 
    }
    // ... etc
};
```

#### 5.3 Update C++ Implementation

```cpp
// MidiPort.cpp

#include "MidiPort.h"
#include "midi_port_cxx.h"

MidiPort::MidiPort(bool track_notes, bool track_controls, bool track_programs)
    : m_rust_port(make_midi_port(track_notes, track_controls, track_programs))
{}

MidiPort::~MidiPort() = default;

uint32_t MidiPort::n_notes_active() const {
    return m_rust_port->n_notes_active();
}

// ... etc
```

## Thread Safety Considerations

### Atomic Operations

The C++ code uses `std::atomic` for counters and mute state. In Rust:
- Use `AtomicU32`, `AtomicU64` for counters
- Use `AtomicBool` for simple boolean flags
- Use `Mutex<T>` or `RwLock<T>` for more complex state

### Shared Ownership

The C++ code uses `shoop_shared_ptr<T>` (based on `std::shared_ptr`). In Rust:
- Use `Arc<T>` for shared ownership (single-threaded reference count)
- Use `Arc<Mutex<T>>` or `Arc<RwLock<T>>` for shared mutable access
- Use `std::rc::Rc<T>` for single-threaded shared ownership

### CXX Bridge Patterns

For crossing the C++/Rust boundary:

```rust
// Use Box<T> for heap allocation on Rust side
fn new_midi_port(...) -> Box<MidiPort> { ... }

// Use &mut T for mutable references
fn process(port: &mut MidiPort, n_frames: u32) { ... }

// Use &T for immutable references
fn name(port: &MidiPort) -> String { ... }
```

## Testing Strategy

1. **Unit Tests in Rust**: Write Rust-native tests using `#[cfg(test)]`
2. **CXX Bridge Tests**: Verify C++ can call Rust and vice versa
3. **Integration Tests**: Existing C++ tests should continue to pass
4. **Property-Based Tests**: Consider using `proptest` for MIDI message generation

## Migration Path

### Option A: Parallel Implementation

Keep both C++ and Rust implementations, with a feature flag to select which to use. This allows gradual migration and easy rollback.

```rust
#[cfg(feature = "rust_midi_ports")]
mod rust_impl;

#[cfg(not(feature = "rust_midi_ports"))]
mod cpp_bridge;
```

### Option B: Direct Replacement

Replace C++ implementations one class at a time, updating the CXX bridge for each.

### Option C: Wrapper Pattern

Create Rust implementations that wrap the existing C++ code initially, then gradually move logic into Rust.

## File Mapping

| C++ File | Rust File | Purpose |
|----------|-----------|---------|
| `MidiPortBase.h/cpp` | `midi_port_base.rs` | Core logic holder |
| `MidiPortBase.h/cpp` | `midi_port_base_cxx.rs` | CXX bridge |
| `MidiPort.h/cpp` | `midi_port.rs` | Main port implementation |
| `MidiPort.h/cpp` | `midi_port_cxx.rs` | CXX bridge |
| `DummyMidiPort.h/cpp` | `dummy_midi_port.rs` | Test port |
| `DummyMidiPort.h/cpp` | `dummy_midi_port_cxx.rs` | CXX bridge |
| `IMidiStateTracking.h` | `midi_traits.rs` | State tracking trait |
| `IMidiRingbuffer.h` | `midi_traits.rs` | Ringbuffer ops trait |
| `IMidiReadableBuffer.h` | `midi_buffer_trait.rs` | Readable buffer trait |
| `IMidiWriteableBuffer.h` | `midi_buffer_trait.rs` | Writeable buffer trait |
| `MidiBuffer.h` | N/A (use traits) | Buffer implementations |
| `MidiBufferingInputPort.h/cpp` | `midi_buffering_input_port.rs` | Buffered input |

## Dependencies

The Rust implementation will depend on:
- `cxx` - CXX bridge
- `arc-swap` or `parking_lot` - For efficient atomics
- `crossbeam` - For concurrent data structures (if needed)
- Existing `backend_rust` dependencies (`log`, etc.)

## Notes

- Keep the C++ interface stable during migration
- Use the CXX `include_cxx!` and `include_rust!` macros appropriately
- Consider using `unsafe` blocks only at the FFI boundary
- Leverage Rust's type system for compile-time guarantees where possible