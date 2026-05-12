# Implementation Task: Port MidiTrackers to Idiomatic Rust

## Summary

Port the `MidiStateTracker` and `MidiStateDiffTracker` C++ classes to idiomatic Rust using a channel-based architecture for tracker-to-diff-tracker communication, replacing the current broken raw-pointer implementation.

## Background

See the following documents for context:
- **`cpp_design.md`** - Documents the original C++ design with shared pointers, callbacks, and synchronous notification
- **`rust_design_current.md`** - Documents the current broken Rust implementation with its issues
- **`rust_design_new.md`** - Documents the proposed channel-based Rust design

## Problem Statement

The current Rust implementation (`src/rust/backend_rust/src/midi_state_tracker.rs` and `src/rust/backend_rust/src/midi_state_diff_tracker.rs`) has fundamental problems:

1. **Missing subscriptions**: `reset()` doesn't call subscribe/unsubscribe
2. **Borrow conflicts**: Can't hold `&mut self` and pass references simultaneously
3. **Raw pointers**: No ownership semantics, manual cleanup required
4. **Mutex blocking**: Not suitable for real-time audio

The C++ implementation passes all 139 tests. The Rust implementation fails.

## Existing Code Context

There is **already a failing Rust implementation** that should be **modified or replaced**:

| File | Status | Notes |
|------|--------|-------|
| `src/rust/backend_rust/src/midi_state_tracker.rs` | Replace | Broken - rewrite needed |
| `src/rust/backend_rust/src/midi_state_diff_tracker.rs` | Replace | Broken - rewrite needed |
| `src/rust/backend_rust/src/cxx.rs` | **Partially reusable** | Contains working FFI bindings for C++ interop; methods signatures may change |
| `src/rust/backend_rust/Cargo.toml` | Extend | May need crossbeam-channel dependency added |
| `src/backend/internal/MidiStateTracker.cpp` | Reusable | C++ wrapper should mostly work |
| `src/backend/internal/MidiStateDiffTracker.cpp` | Reusable | C++ wrapper should mostly work |

### What is Reusable from cxx.rs

The `cxx::bridge` FFI infrastructure in `cxx.rs` should be **reused and extended**, not replaced:
- The `mod ffi` block structure
- The C++ interop types (`Box<T>`, `Vec<u8>`, raw pointers where needed)
- The wrapper functions that call into the Rust implementations

**What changes**: The method signatures exposed through cxx may need updates to match the new architecture (e.g., the new `sync()` method, changed `subscribe()` signature).

## Solution Architecture

### Overview

Replace raw pointer callbacks with `crossbeam-channel` for lock-free notification:

```
Tracker                                          DiffTracker
  │                                                  │
  │  state_data: Arc<TrackerStateData> ────────────►│ Arc::clone() for reading
  │                                                  │
  │  sender: Sender<Event> ─────────────────────────►│ receiver: Receiver<Event>
  │                                                  │
  │  process_msg(data):                              │
  │    ├─ Update state (in-place, no allocation)   │
  │    └─ sender.try_send(event) (atomic, fast)    │
  │                                                  │
  │                              resolve_to(target): │
  │                                ├─ sync() drains │
  │                                │   all channels │
  │                                └─ Generate msgs │
```

### Key Design Points

1. **No raw pointers** between trackers and diff trackers - only IDs
2. **Lock-free notification** via pre-allocated channels
3. **Shared state via Arc** for diff trackers to read tracker state
4. **Deferred processing** - `sync()` drains all channels before `resolve_to()`
5. **Real-time safe** - no allocations or mutexes during processing

See `rust_design_new.md` for detailed design.

## Implementation Tasks

### Phase 1: Core Data Structures

#### 1.1 Define Event Type
```rust
// New file: src/rust/backend_rust/src/event.rs

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub tracker_id: u64,
    pub event_type: u8,    // 0=note, 1=cc, 2=program, 3=pitch, 4=pressure
    pub channel: u8,
    pub param1: u8,        // note or CC number
    pub param2: u8,        // velocity or value
}

pub const EVENT_NOTE: u8 = 0;
pub const EVENT_CC: u8 = 1;
pub const EVENT_PROGRAM: u8 = 2;
pub const EVENT_PITCH: u8 = 3;
pub const EVENT_PRESSURE: u8 = 4;
```

#### 1.2 Define TrackerStateData
```rust
// Shared immutable view of tracker state for diff trackers

#[derive(Clone)]
pub struct TrackerStateData {
    pub notes_active: Vec<u8>,         // 16 * 128
    pub controls: Vec<u8>,             // 16 * 128
    pub programs: Vec<u8>,             // 16
    pub pitch_wheel: Vec<u16>,         // 16
    pub channel_pressure: Vec<u8>,     // 16
}
```

#### 1.3 Add crossbeam-channel dependency
```toml
# src/rust/backend_rust/Cargo.toml
[dependencies]
crossbeam-channel = "0.5"
```

### Phase 2: MidiStateTracker Rewrite

#### 2.1 New Structure
```rust
// src/rust/backend_rust/src/midi_state_tracker.rs

pub struct MidiStateTracker {
    id: u64,
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    
    // Owned mutable state
    notes_active: Vec<u8>,
    controls: Vec<u8>,
    programs: Vec<u8>,
    pitch_wheel: Vec<u16>,
    channel_pressure: Vec<u8>,
    n_notes_active: AtomicI32,
    
    // Shared immutable view
    state_data: Arc<TrackerStateData>,
    
    // Lock-free channel for notifications
    sender: Sender<Event>,
}
```

#### 2.2 process_msg Implementation
- Parse MIDI message
- Update state in-place
- Call `sender.try_send(event)` if state changed
- **No allocation, no mutex**

#### 2.3 Subscription Method
```rust
pub fn create_subscription(&self) -> (Sender<Event>, Arc<TrackerStateData>) {
    // Returns channel sender clone and shared state for diff tracker
}
```

### Phase 3: MidiStateDiffTracker Rewrite

#### 3.1 New Structure
```rust
// src/rust/backend_rust/src/midi_state_diff_tracker.rs

pub struct MidiStateDiffTracker {
    id: u64,
    
    subscriptions: Vec<Subscription>,
    
    diffs: BTreeSet<[u8; 2]>,
    
    tracker_a: Option<(u64, Arc<TrackerStateData>)>,
    tracker_b: Option<(u64, Arc<TrackerStateData>)>,
}

struct Subscription {
    tracker_id: u64,
    receiver: Receiver<Event>,
    state: Arc<TrackerStateData>,
}
```

#### 3.2 reset Implementation
```rust
pub fn reset(&mut self, a: &MidiStateTracker, b: &MidiStateTracker) {
    // Clear old subscriptions
    self.subscriptions.clear();
    
    // Set up new subscriptions via channels
    let (tx_a, rx_a) = bounded(EVENT_CHANNEL_CAPACITY);
    let (tx_b, rx_b) = bounded(EVENT_CHANNEL_CAPACITY);
    
    // Subscribe trackers
    a.add_receiver(rx_a);
    b.add_receiver(rx_b);
    
    self.subscriptions.push(Subscription { tracker_id: a.id(), receiver: rx_a, state: a.state_data() });
    self.subscriptions.push(Subscription { tracker_id: b.id(), receiver: rx_b, state: b.state_data() });
    
    self.tracker_a = Some((a.id(), Arc::clone(&a.state_data)));
    self.tracker_b = Some((b.id(), Arc::clone(&b.state_data)));
    
    // Initial sync
    self.sync();
}
```

#### 3.3 sync Implementation
```rust
fn sync(&mut self) {
    // Drain ALL receivers completely
    for sub in &mut self.subscriptions {
        while let Ok(event) = sub.receiver.try_recv() {
            self.process_event(&sub.state, event);
        }
    }
}
```

#### 3.4 resolve_to Implementation
```rust
pub fn resolve_to(&mut self, target_id: u64) -> Vec<u8> {
    // CRITICAL: Always sync first
    self.sync();
    
    // Get "from" tracker state
    let from_state = self.get_other_state(target_id);
    
    // Generate messages
    let mut out = Vec::new();
    for diff in &self.diffs {
        // Look up values in from_state, generate messages
    }
    out
}
```

### Phase 4: CXX Bindings Update

#### 4.1 Update ffi module
```rust
// src/rust/backend_rust/src/cxx.rs

#[cxx::bridge]
mod ffi {
    extern "Rust" {
        type MidiStateTracker;
        fn new_midi_state_tracker(...) -> Box<MidiStateTracker>;
        // ... other methods ...
        
        // New subscription-based methods
        unsafe fn create_subscription(self: &MidiStateTracker) -> SubscriptionHandle;
    }
    
    extern "Rust" {
        type MidiStateDiffTracker;
        // ... existing methods ...
        
        // Update reset to work with new architecture
        unsafe fn reset(self: &mut MidiStateDiffTracker, a: &mut MidiStateTracker, b: &mut MidiStateTracker);
    }
}

// SubscriptionHandle wraps the channel sender and Arc
struct SubscriptionHandle {
    sender: Sender<Event>,
    state: Arc<TrackerStateData>,
}
```

#### 4.2 Update C++ Wrapper

The C++ wrappers sit on top of the Rust implementation and provide the C++ API. Most should work unchanged, but verify these aspects:

**MidiStateTracker.cpp**:
- `subscribe()` / `unsubscribe()` calls - should work as-is (Rust now stores channels instead of raw pointers)
- `process_msg()` - passes through to Rust, no change needed

**MidiStateDiffTracker.cpp**:
- `reset()` - **critical**: must set up the channel-based subscriptions by calling Rust methods
- `resolve_to()` - **critical**: C++ calls this; ensure Rust's `sync()` is called first (inside Rust implementation)

**Important**: The C++ code calls `subscribe()` on trackers and `reset()` on diff trackers. With the new architecture:

```cpp
// C++ calls this on tracker:
void MidiStateTracker::subscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    if (s) m_rust->add_subscription(s->raw_ptr());
}

// C++ calls this on diff tracker:
void MidiStateDiffTracker::reset(SharedTracker a, SharedTracker b, ...) {
    // OLD: directly set raw pointers
    // NEW: create channels between trackers and diff tracker
    m_rust->reset(a->raw_ptr(), b->raw_ptr());  // Rust sets up channels internally
}
```

The C++ wrapper calls remain the same, but the Rust implementation changes from storing raw pointers to storing channel senders/receivers.

### Phase 5: Testing

#### 5.1 Unit Tests
Create tests in `src/rust/backend_rust/src/`:
- `midi_state_tracker_test.rs`
- `midi_state_diff_tracker_test.rs`

Test scenarios:
1. Note on/off - verify state updates
2. Subscription - verify channel setup
3. Diff recording - verify events propagate
4. Resolve - verify messages generated correctly

#### 5.2 Integration Tests
Run existing C++ tests:
```bash
target/debug/build/backend-*/out/cmake_build/build/test/test_runner
```

Verify all 139 tests pass with Rust implementation.

## Files to Modify

> **Note**: These are the existing files with broken implementations. Replace their contents with the new channel-based implementations.

| File | Action | Notes |
|------|--------|-------|
| `src/rust/backend_rust/Cargo.toml` | Extend | Add `crossbeam-channel = "0.5"` dependency |
| `src/rust/backend_rust/src/midi_state_tracker.rs` | **Replace** | Complete rewrite - see Phase 2 |
| `src/rust/backend_rust/src/midi_state_diff_tracker.rs` | **Replace** | Complete rewrite - see Phase 3 |
| `src/rust/backend_rust/src/cxx.rs` | Update | Keep FFI infrastructure, update method signatures |
| `src/backend/internal/MidiStateTracker.cpp` | Verify | Should work as-is; verify `subscribe()` calls |
| `src/backend/internal/MidiStateDiffTracker.cpp` | Verify | Should work as-is; verify `reset()` sets up channels |

## Files to Create

> **Note**: These are new files to be added to the existing crate.

| File | Purpose |
|------|---------|
| `src/rust/backend_rust/src/event.rs` | Event type definition for channel communication |
| `src/rust/backend_rust/src/midi_state_tracker_test.rs` | Unit tests for tracker |
| `src/rust/backend_rust/src/midi_state_diff_tracker_test.rs` | Unit tests for diff tracker |

## Verification Checklist

- [ ] Rust compiles without errors
- [ ] No compiler warnings about unsafe code
- [ ] `cargo test` passes for new unit tests
- [ ] All 139 C++ integration tests pass
- [ ] No memory leaks detected
- [ ] Real-time safety verified (no allocations in process_msg path)

## References

- Original C++ design: `cpp_design.md`
- Current broken implementation: `rust_design_current.md`  
- Proposed new design: `rust_design_new.md`

## Consulting Existing Code

When implementing, **consult these existing files for reference**:

1. **`src/rust/backend_rust/src/midi_state_tracker.rs`** - Current broken implementation; contains:
   - MIDI parsing logic (can be reused)
   - State storage patterns
   - Index calculation (`note_index`, `cc_index`)
   - MIDI helper usage (`midi_helpers` module)

2. **`src/rust/backend_rust/src/midi_helpers.rs`** - If exists, contains MIDI parsing helpers

3. **`src/backend/internal/MidiStateTracker.cpp`** - Original C++ implementation; reference for:
   - Correct behavior of `process_msg()`
   - State update logic
   - Helper function implementations

4. **`src/backend/internal/MidiStateDiffTracker.cpp`** - Original C++ implementation; reference for:
   - Correct diff computation logic
   - `resolve_to()` message generation
   - `rescan_diff()` implementation

**Do not copy-paste** the broken Rust code directly. Use the C++ and the design documents as the source of truth for behavior.
