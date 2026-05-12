# Current Rust MidiTracker Design Document

## Overview

The current Rust implementation attempts to replicate the C++ design using raw pointers and CXX bridge for FFI. It struggles with Rust's ownership and lifetime rules.

## MidiStateTracker (Current)

### Source
`src/rust/backend_rust/src/midi_state_tracker.rs`

### Structure
```rust
pub struct MidiStateTracker {
    id: u64,
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    n_notes_active: AtomicI32,
    notes_active_velocities: Vec<u8>,
    controls: Vec<u8>,
    programs: Vec<u8>,
    pitch_wheel: Vec<u16>,
    channel_pressure: Vec<u8>,
    // PROBLEM: Raw pointers for callbacks
    subscribers: Mutex<Vec<(*mut MidiStateDiffTracker, u64)>>,
}
```

### Key Issues

#### Issue 1: Raw Pointers for Subscribers
```rust
subscribers: Mutex<Vec<(*mut MidiStateDiffTracker, u64)>>
```

The tracker holds raw pointers to diff trackers. This is:
- **Unsafe**: No lifetime guarantees
- **Manual**: Drop implementation must clean up
- **Not idiomatic Rust**: Fights the borrow checker

#### Issue 2: Mutex for Notification
```rust
fn notify_note_changed(&self, channel: u8, note: u8, maybe_velocity: Option<u8>) {
    let subs = self.subscribers.lock().unwrap().clone();
    for (sub, _) in subs {
        unsafe { (*sub).note_changed(self.id, self, channel, note, maybe_velocity); }
    }
}
```

- **Mutex required**: Because subscribers are shared
- **Clone on every notification**: Performance cost
- **Not real-time safe**: Mutex can block

#### Issue 3: Borrow Checker Conflicts
```rust
fn process_note_on(&mut self, channel: u8, note: u8, velocity: u8) {
    // &mut self held here
    if self.notes_active_velocities[idx] != velocity {
        self.notify_note_changed(channel, note, Some(velocity));  // Takes &self!
        // ERROR: cannot borrow `*self` as mutable and immutable simultaneously
    }
}
```

The notification method takes `&self`, but `process_note_on` has `&mut self`.

### CXX Bindings

The CXX bridge exposes:
```rust
unsafe fn subscribe(self: &mut MidiStateTracker, subscriber: *mut MidiStateDiffTracker);
unsafe fn unsubscribe(self: &mut MidiStateTracker, subscriber: *mut MidiStateDiffTracker);
```

C++ calls `subscribe()` which:
1. Adds raw pointer to subscribers list
2. Calls `track_subscription()` on the diff tracker

## MidiStateDiffTracker (Current)

### Source
`src/rust/backend_rust/src/midi_state_diff_tracker.rs`

### Structure
```rust
pub struct MidiStateDiffTracker {
    id: u64,
    diffs: Mutex<BTreeSet<[u8; 2]>>,
    // PROBLEM: Raw pointers to trackers
    a: AtomicPtr<MidiStateTracker>,
    b: AtomicPtr<MidiStateTracker>,
    a_id: AtomicU64,
    b_id: AtomicU64,
    // For cleanup
    subscribed_to: Mutex<Vec<*mut MidiStateTracker>>,
}
```

### Key Issues

#### Issue 1: Raw Pointers to Trackers
```rust
a: AtomicPtr<MidiStateTracker>,
b: AtomicPtr<MidiStateTracker>,
```

- No ownership semantics
- No guarantee of validity
- Must be manually managed

#### Issue 2: Unsafe Borrow in Callback
```rust
pub fn note_changed(&self, tracker_id: u64, ...) {
    let Some(other) = self.other_tracker_by_id(tracker_id) else { return };
    // `other` is &mut MidiStateTracker, but `self` is still borrowed as &self
    // ERROR: cannot borrow `self` as mutable while borrowed as immutable
}
```

#### Issue 3: Unsafe Borrow in resolve_to
```rust
pub fn resolve_to(&mut self, to: *mut MidiStateTracker, ...) -> Vec<u8> {
    let to = unsafe { &mut *to };           // &mut MidiStateTracker
    let Some(a) = self.get_a() else { ... }; // Also needs &mut self!
    let Some(b) = self.get_b() else { ... }; // Can't have two &mut borrows!
    
    // Both `to` and `a`/`b` are &mut - overlapping borrows!
}
```

### Current Reset Implementation

```rust
pub fn reset(&mut self, a: *mut MidiStateTracker, b: *mut MidiStateTracker) {
    // NO unsubscribe from old trackers!
    // NO subscribe to new trackers!
    
    self.a.store(a, Ordering::Relaxed);
    self.b.store(b, Ordering::Relaxed);
    self.diffs.lock().unwrap().clear();
}
```

**Critical bug**: Does not call subscribe/unsubscribe. Trackers never know they're being tracked.

## C++ Side (Current Wrapper)

### MidiStateTracker.cpp
```cpp
void MidiStateTracker::subscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    if (s) m_rust->subscribe(s->raw_ptr());  // Passes raw pointer
}

void MidiStateTracker::unsubscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    if (s) m_rust->unsubscribe(s->raw_ptr());
}
```

### MidiStateDiffTracker.cpp
```cpp
void MidiStateDiffTracker::reset(SharedTracker a, SharedTracker b, StateDiffTrackerAction action) {
    // Unsubscribe from old
    if (m_a) m_a->raw_ptr()->unsubscribe(m_rust.operator->());
    if (m_b) m_b->raw_ptr()->unsubscribe(m_rust.operator->());
    
    m_a = a;
    m_b = b;
    
    // Subscribe to new
    if (m_a) m_a->raw_ptr()->subscribe(m_rust.operator->());
    if (m_b) m_b->raw_ptr()->subscribe(m_rust.operator->());
    
    m_rust->reset(m_a ? m_a->raw_ptr() : nullptr, ...);
}
```

**The problem**: The Rust code doesn't have a `subscribe()` method that does anything - it was removed but C++ still calls it.

## Summary of Current Problems

| Problem | Impact |
|---------|--------|
| Raw pointers to trackers | Lifetime issues, use-after-free potential |
| Raw pointers to diff trackers | Subscription management broken |
| Missing subscribe/unsubscribe | Trackers never notify diff trackers |
| Mutex on hot path | Not real-time safe |
| Borrow conflicts | Won't compile cleanly |
| Clone on notification | Performance overhead |

## Why It Doesn't Work

1. **Borrow checker**: Can't have `&mut self` and `&self` simultaneously
2. **No subscriptions**: `reset()` doesn't call subscribe
3. **Raw pointers everywhere**: No ownership semantics, manual cleanup required
4. **Mutex blocking**: Not suitable for real-time audio

## References

- Current code: `src/rust/backend_rust/src/midi_state_tracker.rs`, `src/rust/backend_rust/src/midi_state_diff_tracker.rs`
- C++ wrapper: `src/backend/internal/MidiStateTracker.cpp`, `src/backend/internal/MidiStateDiffTracker.cpp`
- Original design: See `cpp_design.md`
