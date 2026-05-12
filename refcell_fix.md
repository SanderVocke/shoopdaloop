# Fix: MidiStateDiffTracker Resolution Using Live Tracker State

## Background

This document describes a bug fix for the Rust port of the MidiStateTracker and MidiStateDiffTracker components from C++.

### Project Context

- **Repository:** The project lives in `/home/sander/dev/shoopdaloop-2` (working branch)
- **C++ Reference:** The original C++ implementation is available in `/home/sander/dev/shoopdaloop` (master branch)
- **What is being ported:** The `MidiStateTracker` and `MidiStateDiffTracker` C++ classes track MIDI state (notes, CC, pitch wheel, etc.) and compute differences between two tracker states for loop playback functionality.
- **Architecture:** The Rust implementation uses a channel-based architecture where trackers communicate state changes to diff trackers via `crossbeam-channel`. The C++ version uses callback-based subscriptions.

### Building and Testing

The entire project is built with:
```bash
cargo build
```

This compiles both the Rust code and the embedded C++ backend (via CMake). The test runner executable is located at:
```
./target/debug/build/backend-5ec9784d889b375a/out/cmake_build/build/test/test_runner
```

### Comparing Implementations

To compare the C++ and Rust implementations:

**C++ (working):**
```bash
cd ~/dev/shoopdaloop
cargo build
~/dev/shoopdaloop/target/debug/build/backend-b8b705b49d0e5add/out/cmake_build/build/test/test_runner "AudioMidiLoop - Midi - CC State tracking"
```

**Rust (broken):**
```bash
cargo build
./target/debug/build/backend-5ec9784d889b375a/out/cmake_build/build/test/test_runner "AudioMidiLoop - Midi - CC State tracking"
```

The Rust version fails 4 tests while the C++ version passes all tests.

### Debugging Notes

Debug output was added to both implementations to trace the issue:
- Rust: `eprintln!("[RUST] ...")` calls
- C++: `std::cerr << "[CPP] ..."` calls

## Problem Summary

The Rust implementation of `MidiStateDiffTracker` fails the "AudioMidiLoop - Midi - CC State tracking" test because it reads from **stale snapshots** of tracker state instead of **live state**.

### Root Cause

When `reset()` is called, the Rust implementation captures a snapshot of each tracker's state:

```rust
// Current broken code in reset()
self.tracker_a = Some((a.get_id(), Arc::new(a.get_state_data())));  // Snapshot!
self.tracker_b = Some((b.get_id(), Arc::new(b.get_state_data())));  // Snapshot!
```

Later, when `resolve_to()` is called, it reads from these snapshots:

```rust
// Current broken code in resolve_to_by_id()
let from_state = self.get_other_state(target_id);  // Returns SNAPSHOT
let pitch = from_state.maybe_pitch_wheel_value(channel);  // Reads stale value!
```

The C++ implementation works because it reads directly from live tracker objects via references:

```cpp
// C++ works because it reads from live objects
auto &from = to == m_a.get() ? *m_b : *m_a;
auto pw_from = from.maybe_pitch_wheel_value(channel);  // Live value!
```

### Evidence from Debug Output

**C++ (working):**
```
[CPP]   processing diff [0xe0, 0]
    -> pitch wheel from=49           // Current value
    -> put_message_cb(3, [0xe0, 31, 0])  // Correct output
```

**Rust (broken):**
```
[RUST]   tracker_a pitch_wheel[0]=150        // Snapshot from reset()
[RUST]   tracker_b pitch_wheel[0]=8192      // Snapshot from reset()
    -> pitch from 'from': 8192 (0x2000)    // Stale value!
    -> output pitch [0xe0, 0, 64]          // Wrong output!
```

---

## Solution: Rc<RefCell<T>> Pattern

### Why Rc<RefCell<T>>

The C++ code works because:
1. It stores direct references (`&`) to the trackers
2. `resolve_to()` is called with both `to` and implicitly `from`
3. It reads directly from the live objects

In Rust, we cannot safely store references that outlive their lifetime. The alternatives are:

| Approach | Problem |
|----------|---------|
| Raw pointers | Unsafe, not Rust-idiomatic |
| Global registry with IDs | IDs can dangle, global state |
| `Arc<TrackerStateData>` snapshots | Stale data (current bug) |
| `Rc<RefCell<T>>` | Shared ownership, runtime borrow checking |

`Rc<RefCell<T>>` is Rust's standard pattern for:
- Multiple owners of the same data
- Mutation through one owner at a time
- Runtime borrow checking (acceptable in single-threaded context)

This matches the C++ semantics where `resolve_to()` temporarily borrows the trackers while computing the result.

### Architecture Change

**Before (broken):**
```
MidiStateDiffTracker
├── tracker_a: Option<(u64, Arc<TrackerStateData>)>  // Snapshot!
├── tracker_b: Option<(u64, Arc<TrackerStateData>)>  // Snapshot!
└── subscriptions: Vec<Subscription>

MidiStateTracker
└── (standalone, not referenced by diff tracker)
```

**After (fixed):**
```
MidiStateDiffTracker
├── tracker_a: Rc<RefCell<MidiStateTracker>>  // Shared ownership
├── tracker_b: Rc<RefCell<MidiStateTracker>>  // Shared ownership
└── subscriptions: Vec<Subscription>

MidiStateTracker
└── (can be shared via Rc with diff tracker)
```

---

## Implementation Steps

### 1. Modify `MidiStateTracker` Struct

**File:** `src/rust/backend_rust/src/midi_state_tracker.rs`

The `MidiStateTracker` itself doesn't need major changes, but we need to ensure it can be wrapped in `Rc<RefCell<>>`. The existing struct should work fine since `RefCell` provides the interior mutability.

### 2. Modify `MidiStateDiffTracker` Struct

**File:** `src/rust/backend_rust/src/midi_state_diff_tracker.rs`

Change the struct fields:

```rust
// OLD (broken)
use std::sync::Arc;
use crate::midi_state_tracker::MidiStateTracker;

pub struct MidiStateDiffTracker {
    id: u64,
    subscriptions: Vec<Subscription>,
    diffs: BTreeSet<[u8; 2]>,
    tracker_a: Option<(u64, Arc<TrackerStateData>)>,  // Snapshot!
    tracker_b: Option<(u64, Arc<TrackerStateData>)>,  // Snapshot!
}

// NEW (fixed)
use std::cell::RefCell;
use std::rc::Rc;
use crate::midi_state_tracker::MidiStateTracker;

pub struct MidiStateDiffTracker {
    id: u64,
    subscriptions: Vec<Subscription>,
    diffs: BTreeSet<[u8; 2]>,
    tracker_a: Option<Rc<RefCell<MidiStateTracker>>>,  // Live reference!
    tracker_a_id: u64,  // Store ID separately for logging/debugging
    tracker_b: Option<Rc<RefCell<MidiStateTracker>>>,  // Live reference!
    tracker_b_id: u64,
}
```

### 3. Update Subscription Struct

The `Subscription` struct needs to reference live trackers via `Rc`:

```rust
// OLD
struct Subscription {
    tracker_id: u64,
    receiver: Receiver<Event>,
    state: Arc<TrackerStateData>,  // Snapshot
}

// NEW
struct Subscription {
    tracker_id: u64,
    receiver: Receiver<Event>,
    tracker: Rc<RefCell<MidiStateTracker>>,  // Live reference
}
```

### 4. Update `reset()` Method

```rust
// OLD (broken)
pub fn reset(&mut self, a: &mut MidiStateTracker, b: &mut MidiStateTracker) {
    self.subscriptions.clear();
    
    // Store snapshots
    self.tracker_a = Some((a.get_id(), Arc::new(a.get_state_data())));
    self.tracker_b = Some((b.get_id(), Arc::new(b.get_state_data())));
    
    // Create subscriptions
    let rx_a = a.create_subscription();
    let rx_b = b.create_subscription();
    // ... rest of subscription setup
}

// NEW (fixed)
pub fn reset(&mut self, a: Rc<RefCell<MidiStateTracker>>, b: Rc<RefCell<MidiStateTracker>>) {
    self.subscriptions.clear();
    
    // Store shared ownership
    self.tracker_a = Some(Rc::clone(&a));
    self.tracker_a_id = a.borrow().get_id();
    self.tracker_b = Some(Rc::clone(&b));
    self.tracker_b_id = b.borrow().get_id();
    
    // Create subscriptions using the live tracker
    let mut a_mut = a.borrow_mut();
    let mut b_mut = b.borrow_mut();
    let rx_a = a_mut.create_subscription();
    let rx_b = b_mut.create_subscription();
    
    self.subscriptions.push(Subscription {
        tracker_id: a_mut.get_id(),
        receiver: rx_a,
        tracker: Rc::clone(&a),
    });
    self.subscriptions.push(Subscription {
        tracker_id: b_mut.get_id(),
        receiver: rx_b,
        tracker: Rc::clone(&b),
    });
    
    // Initial sync
    self.sync();
    self.compare_static_states();
}
```

### 5. Update `get_other_state()` Method

```rust
// OLD (broken) - returns snapshot
fn get_other_state(&self, tracker_id: u64) -> Option<Arc<TrackerStateData>> {
    if let Some((id, state)) = &self.tracker_a {
        if *id == tracker_id {
            return Some(Arc::clone(state));  // Return snapshot
        }
    }
    // ...
}

// NEW (fixed) - returns live reference
fn get_other_state(&self, tracker_id: u64) -> Option<Rc<RefCell<MidiStateTracker>>> {
    if let Some(ref tracker) = self.tracker_a {
        if tracker.borrow().get_id() == tracker_id {
            return Some(Rc::clone(tracker));  // Return live reference
        }
    }
    if let Some(ref tracker) = self.tracker_b {
        if tracker.borrow().get_id() == tracker_id {
            return Some(Rc::clone(tracker));
        }
    }
    None
}
```

### 6. Update `sync()` and `process_event_internal()` Methods

The `sync()` method needs to work with live trackers. When processing events, we read from the live tracker state:

```rust
fn sync(&mut self) {
    // ... collect events as before ...
    
    // Process events using live tracker state
    for (state, event) in events {
        // state is the Rc<RefCell<MidiStateTracker>>
        self.process_event_internal(&state, event);
    }
}

fn process_event_internal(&self, tracker: &Rc<RefCell<MidiStateTracker>>, event: Event) {
    let tracker_data = tracker.borrow();  // Immutable borrow
    
    // Read live state
    let pitch = tracker_data.maybe_pitch_wheel_value(channel);
    // ...
}
```

### 7. Update `resolve_to_by_id()` Method

```rust
// OLD - reads from snapshot
fn resolve_to_by_id(&mut self, target_id: u64, ...) -> Vec<u8> {
    self.sync();
    let from_state = self.get_other_state(target_id);  // Snapshot
    let pitch = from_state.maybe_pitch_wheel_value(channel);  // Stale!
    // ...
}

// NEW - reads from live tracker
fn resolve_to_by_id(&mut self, target_id: u64, ...) -> Vec<u8> {
    self.sync();
    
    let from_tracker = self.get_other_state(target_id);  // Live reference
    let Some(from) = from_tracker else { return Vec::new(); };
    
    // Borrow the live tracker to read state
    let from_data = from.borrow();  // Immutable borrow
    
    for diff in &self.diffs {
        match diff_kind(diff) {
            // Read from LIVE state
            EVENT_PITCH => {
                let pitch = from_data.maybe_pitch_wheel_value(channel);  // Current!
                // ...
            }
            // ...
        }
    }
}
```

### 8. Update C++ FFI Wrapper

**File:** `src/backend/internal/MidiStateDiffTracker.cpp`

The C++ wrapper needs to create `Rc<RefCell<MidiStateTracker>>` and pass it to Rust:

```cpp
// In the wrapper's reset() method
void MidiStateDiffTracker::reset(SharedTracker a, SharedTracker b, StateDiffTrackerAction action) {
    m_a = a;
    m_b = b;
    if (m_a && m_b) {
        // Create Rc wrappers and pass to Rust
        auto rc_a = make_rc_ref(m_a.get());  // Need helper to create Rc<RefCell<>>
        auto rc_b = make_rc_ref(m_b.get());
        m_rust->reset(rc_a, rc_b);
    }
    // ...
}
```

This requires creating a helper function or modifying the Rust FFI to accept raw pointers and create the Rc inside.

**Alternative:** Modify the FFI to accept raw pointers and construct `Rc` in Rust:

```rust
// In cxx.rs or lib.rs
#[no_mangle]
pub extern "C" fn reset_with_ptrs(
    this: &mut MidiStateDiffTracker,
    a: *mut MidiStateTracker,
    b: *mut MidiStateTracker,
) {
    let rc_a = unsafe { Rc::new(RefCell::new(std::ptr::read(a))) };
    let rc_b = unsafe { Rc::new(RefCell::new(std::ptr::read(b))) };
    this.reset(rc_a, rc_b);
}
```

**Note:** The FFI changes are tricky because `Rc` is not `Send` and cannot cross FFI boundaries directly. You may need to:
1. Use raw pointers in the FFI layer
2. Create `Rc` wrappers internally in Rust
3. Or restructure how trackers are created/passed

### 9. Update `TrackerStateData` Usage

Search for all uses of `TrackerStateData` and replace with `Rc<RefCell<MidiStateTracker>>`:

- `reset()` - stores `Rc` instead of `Arc<TrackerStateData>`
- `subscriptions` - stores `Rc` instead of `Arc<TrackerStateData>`
- `get_state_data()` calls - no longer needed in most places
- Any code that reads from `Arc<TrackerStateData>` - now reads from `Rc<RefCell<MidiStateTracker>>`

### 10. Update `midi_state_tracker.rs` Methods

Methods like `get_state_data()` may no longer be needed or may need modification:

```rust
// May need to keep for serialization or debugging
pub fn get_state_data(&self) -> TrackerStateData {
    // Still useful for copying state somewhere
    TrackerStateData {
        notes_active: self.notes_active.clone(),
        controls: self.controls.clone(),
        // ...
    }
}
```

---

## Verification Commands

### Build the Project

```bash
cd /home/sander/dev/shoopdaloop-2
cargo build 2>&1 | tail -20
```

### Run the Failing Test Case

```bash
./target/debug/build/backend-5ec9784d889b375a/out/cmake_build/build/test/test_runner "AudioMidiLoop - Midi - CC State tracking"
```

### Expected Output (after fix)

```
All tests passed (703 assertions in 1 test case)
```

### Run All Tests

```bash
./target/debug/build/backend-5ec9784d889b375a/out/cmake_build/build/test/test_runner
```

Expected: **All tests pass** (was 105 passed, 4 failed)

### Debugging Notes

If issues arise, add debug prints using `eprintln!()`:

```rust
// In resolve_to_by_id()
let from_data = from.borrow();
eprintln!("[DEBUG] Reading pitch from live tracker: {}", from_data.maybe_pitch_wheel_value(0));
```

---

## Files to Modify

1. `src/rust/backend_rust/src/midi_state_diff_tracker.rs` - Main implementation changes
2. `src/rust/backend_rust/src/midi_state_tracker.rs` - Minor changes (create_subscription may need update)
3. `src/rust/backend_rust/src/cxx.rs` - FFI signatures
4. `src/backend/internal/MidiStateDiffTracker.cpp` - C++ wrapper

---

## Summary of Changes

| What | Old | New |
|------|-----|-----|
| Tracker storage | `Option<(u64, Arc<TrackerStateData>)>` | `Option<Rc<RefCell<MidiStateTracker>>>` |
| Snapshot storage | `Arc<TrackerStateData>` | `Rc<RefCell<MidiStateTracker>>` |
| State reading | From snapshot | From live tracker via `.borrow()` |
| Reset signature | `reset(&mut self, a: &mut T, b: &mut T)` | `reset(&mut self, a: Rc<RefCell<T>>, b: Rc<RefCell<T>>)` |
| Thread safety | N/A | `Rc` is single-threaded (acceptable for audio) |

The key insight is that we replace "store a snapshot of state" with "share ownership of the live tracker". The `Rc` ensures the tracker stays alive as long as the diff tracker needs it, and `RefCell` allows mutable access when needed.
