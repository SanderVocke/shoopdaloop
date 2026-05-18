# Plan: Porting MidiPortBase to Rust

## Overview

This plan documents the work to port `MidiPortBase` from C++ to Rust, eliminating the state duplication issue in `MidiPort`.

**Build system**: `cargo build` builds everything (C++ and Rust). C++ unit tests are in `test_runner` executable (in `target/`). Rust tests via `cargo test`.

## Problem Statement

`MidiPort` currently maintains state in TWO places:
1. `rust::Box<backend_rust::MidiPort> m_rust_port` — Rust implementation
2. `MidiPortBase m_base` — C++ implementation

This causes:
- Duplicate state storage (mute, events, ringbuffer)
- Maintenance burden — both must be kept in sync
- Potential synchronization bugs

The goal is to move all state to Rust, eliminate `MidiPortBase` from C++, and make `MidiPort` a thin wrapper delegating to `m_rust_port`.

## Current State

### File Structure
```
src/
├── backend/internal/
│   ├── MidiPort.h / .cpp          # C++ wrapper, contains BOTH m_rust_port AND m_base
│   ├── MidiPortBase.h / .cpp      # C++ base class (to be deleted)
│   └── ...
└── rust/backend_rust/src/
    ├── midi_port.rs               # Rust MidiPort (contains MidiPortBase)
    ├── midi_port_base.rs          # Rust MidiPortBase
    ├── midi_port_cxx.rs           # CXX bridge for MidiPort
    └── midi_port_base_cxx.rs      # CXX bridge for MidiPortBase
```

### Class Relationships
```
C++ MidiPortBase  ←──────────────────  C++ MidiPort (m_base member)
      │                                    (also has m_rust_port)
      │                                         │
      ├── m_muted                              │
      ├── n_input_events, n_output_events      │
      ├── m_midi_ringbuffer (MidiRingbuffer)    │
      ├── m_maybe_midi_state (MidiStateTracker) │
      └── m_ringbuffer_tail_state (MidiStateTracker)
                                                 │
                                                 ▼
                                         Rust midi_port::MidiPort
                                                (contains midi_port_base::MidiPortBase)
```

## Design Decision: Single Source of Truth

**Move `MidiPortBase` fully to Rust. Make C++ `MidiPort` delegate all methods to `m_rust_port`.**

### Rationale
- `MidiPortBase` contains pure state/logic (no I/O, no platform deps)
- Rust is well-suited for atomic counters, ringbuffer ops
- C++ `MidiPort` exists for interface compatibility but can be stripped down

## Target Architecture

```
AFTER PORT:
═══════════════════════════════════════════════════════════════════

┌──────────────────────────────────────────────────────────────┐
│  Rust Side (Single Source of Truth)                          │
│                                                               │
│  midi_port_base.rs                                           │
│  └── MidiPortBase {                                          │
│        tracking_config, maybe_midi_state, ringbuffer_tail,   │
│        midi_time_window, midi_storage,                       │
│        muted, n_input_events, n_output_events                 │
│    }                                                         │
│                                                               │
│  midi_port.rs                                                │
│  └── MidiPort {                                              │
│        base: MidiPortBase                                    │
│        buffers: MidiPortBuffers                              │
│    }                                                         │
└──────────────────────────────────────────────────────────────┘
                              │
                              │ cxx bridge (namespace: backend_rust)
                              ▼
┌──────────────────────────────────────────────────────────────┐
│  C++ Side (Thin Wrapper)                                     │
│                                                               │
│  MidiPort.h / .cpp                                           │
│  └── rust::Box<backend_rust::MidiPort> m_rust_port           │
│      All methods delegate to m_rust_port->method()            │
│                                                               │
│  NO MORE MidiPortBase.h/.cpp (deleted)                       │
│  NO MORE m_base member                                        │
└──────────────────────────────────────────────────────────────┘
```

## Method Mapping

### Naming Convention
Use old names on both sides (C++ and Rust) for compatibility.

| C++ MidiPortBase method | C++ MidiPort caller | Rust bridge method |
|------------------------|---------------------|-------------------|
| `set_muted(bool)` | `m_base.set_muted()` | `set_muted()` |
| `get_muted()` | `m_base.get_muted()` | `get_muted()` |
| `n_notes_active()` | `m_base.n_notes_active()` | `n_notes_active()` |
| `get_n_input_events()` | `m_base.get_n_input_events()` | `get_n_input_events()` |
| `get_n_output_events()` | `m_base.get_n_output_events()` | `get_n_output_events()` |
| `reset_n_input_events()` | `m_base.reset_n_input_events()` | `reset_n_input_events()` |
| `reset_n_output_events()` | `m_base.reset_n_output_events()` | `reset_n_output_events()` |
| `increment_input_events()` | `m_base.increment_input_events()` | `increment_input_events()` |
| `increment_output_events()` | `m_base.increment_output_events()` | `increment_output_events()` |
| `set_n_samples()` | `m_base.set_n_samples()` | `set_n_samples()` |
| `get_n_samples()` | `m_base.get_n_samples()` | `get_n_samples()` |
| `snapshot()` | `m_base.snapshot()` | delegated to ringbuffer |
| `get_current_n_samples()` | `m_base.get_current_n_samples()` | `get_current_n_samples()` |
| `maybe_midi_state_tracker()` | `m_base.maybe_midi_state_tracker()` | `get_maybe_midi_state_tracker()` (returns raw ptr) |
| `maybe_ringbuffer_tail_state_tracker()` | `m_base.maybe_ringbuffer_tail_state_tracker()` | `get_maybe_ringbuffer_tail_state_tracker()` (returns raw ptr) |
| `set_ringbuffer_n_samples()` | `m_base.set_n_samples()` | alias to `set_n_samples()` |

## Required CXX Bridge Additions

### midi_port_cxx.rs
```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        // Existing methods...

        // NEW: Methods currently delegated to MidiPortBase
        fn get_n_input_events(port: &MidiPort) -> u32;
        fn get_n_output_events(port: &MidiPort) -> u32;
        fn increment_input_events(port: &MidiPort, count: u32);
        fn increment_output_events(port: &MidiPort, count: u32);

        // NEW: Direct state tracker access (returns raw pointer as usize)
        // Returns 0 if tracker not present
        // C++ is responsible for lifetime management (same lifetime as MidiPort)
        unsafe fn get_maybe_midi_state_tracker(port: &mut MidiPort) -> usize;
        unsafe fn get_maybe_ringbuffer_tail_state_tracker(port: &mut MidiPort) -> usize;

        // NEW: Interface compatibility
        fn get_current_n_samples(port: &MidiPort) -> u32;
    }
}
```

## Required C++ Changes

### MidiPort.h
```cpp
// BEFORE
class MidiPort : public virtual PortInterface,
                 public virtual IMidiStateTracking,
                 public virtual IMidiRingbuffer {
    rust::Box<backend_rust::MidiPort> m_rust_port;
    MidiPortBase m_base;  // ← REMOVE
    // ... methods
};

// AFTER
class MidiPort : public virtual PortInterface,
                 public virtual IMidiStateTracking,
                 public virtual IMidiRingbuffer {
    rust::Box<backend_rust::MidiPort> m_rust_port;  // ← ALL state lives here

    // Interface methods implemented by delegation
    // IMidiStateTracking: n_notes_active(), get_input_event_count(), get_output_event_count()
    // IMidiRingbuffer: set_n_samples(), get_n_samples(), snapshot(), get_current_n_samples()
};
```

### MidiPort.cpp
```cpp
// BEFORE
void MidiPort::set_muted(bool muted) {
    m_rust_port->set_muted(muted);      // Rust
    m_base.set_muted(muted);            // C++ (duplicate!)
}

// AFTER
void MidiPort::set_muted(bool muted) {
    m_rust_port->set_muted(muted);      // Rust only - single source
}
```

All other methods follow the same pattern.

## Callers of Direct MidiStateTracker Access

### 1. jack/JackMidiPort.cpp:138
```cpp
maybe_midi_state_tracker()->process_msg(elem.bytes);
```
**Action**: Change to use bridge method:
```cpp
auto* tracker = (MidiStateTracker*)m_rust_port->get_maybe_midi_state_tracker();
if (tracker) {
    tracker->process_msg(elem.bytes);
}
```

### 2. MidiChannel.cpp:687
```cpp
if (auto const& state = midiport->maybe_ringbuffer_tail_state_tracker()) {
    // ... use state
}
```
**Action**: Change to use bridge method:
```cpp
auto* state = (MidiStateTracker*)midiport->get_maybe_ringbuffer_tail_state_tracker();
if (state) {
    // ... use state (note: returns raw ptr, not shared_ptr)
}
```
**Note**: C++ code currently uses `shoop_shared_ptr<MidiStateTracker>`. After change, it will receive a raw pointer. Need to wrap in a temporary shared_ptr or adjust logic.

## Interface Decision (IMidiStateTracking, IMidiRingbuffer)

**Decision: Keep C++ interface inheritance temporary.**

- `IMidiStateTracking` and `IMidiRingbuffer` are abstract C++ interfaces
- No other classes depend on them (only `MidiPortBase` and `MidiPort`)
- They don't cause harm — they're passthrough to Rust implementation
- When migrating to all-Rust, these will be replaced by Rust traits (`MidiStateTracking` trait, `MidiRingbufferOps` trait already exist in `midi_traits.rs`)

**Action**: Keep as-is. Document they are temporary and will be deleted when migrating to all-Rust.

## File Changes Summary

```
REMOVE:
- src/backend/internal/MidiPortBase.h
- src/backend/internal/MidiPortBase.cpp

MODIFY:
- src/rust/backend_rust/src/midi_port_cxx.rs    (add bridge methods)
- src/backend/internal/MidiPort.h              (remove m_base, add delegation)
- src/backend/internal/MidiPort.cpp            (delegate all to m_rust_port)
- src/backend/internal/jack/JackMidiPort.cpp    (use bridge for state tracker)
- src/backend/internal/MidiChannel.cpp         (use bridge for tail state tracker)
- src/backend/CMakeLists.txt                   (remove MidiPortBase from build)
```

## Building and Testing

### Build Commands
- **Build everything (C++ and Rust)**: `cargo build` in the top-level directory
- **C++ unit tests**: Built as `test_runner` executable (found in `target/` directory)
- **Rust unit tests**: `cargo test`

### Build System Notes
- The build uses Cargo for both C++ (via `cc` crate) and Rust (via native Cargo)
- CXX bridge code is generated during build via `cxx-build`
- Generated headers are placed in `src/rust/backend_rust/src/` (e.g., `midi_port_cxx.rs.h`)

## Testing Considerations

1. **State consistency tests** (C++): Run `test_runner` to verify `get_muted()`, `get_n_input_events()` return same values as before
2. **Note tracking tests** (C++): Run `test_runner` to verify `n_notes_active()` still works after processing messages
3. **Ringbuffer tests** (C++): Run `test_runner` to verify `set_n_samples()`, `snapshot()` still work
4. **Jack integration tests** (manual): Verify MIDI input/output still works with JACK backend
5. **Rust tests**: `cargo test` to verify Rust-side MidiPort and MidiPortBase logic

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| **Atomic operations across FFI** | Rust `AtomicBool`/`AtomicU32` are same semantics as C++ - no change needed |
| **`MidiStateTracker` pointer lifetime** | C++ holds `rust::Box<backend_rust::MidiPort>` which owns all state trackers. Raw pointers remain valid as long as `m_rust_port` exists. |
| **Performance of FFI calls** | These are simple state queries (no per-message FFI), acceptable overhead |
| **shared_ptr vs raw pointer** | Some callers expect `shoop_shared_ptr`. Need to either: (a) wrap raw pointer, or (b) adjust caller logic |