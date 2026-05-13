# Plan: Rearchitect Rust MIDI State Tracker Port to Match C++ Behavior

## Context

A port of two C++ classes (`MidiStateTracker` and `MidiStateDiffTracker`) to Rust was undertaken as part of a broader effort to move core audio/MIDI logic into Rust. The port is **mostly complete** but **not behaviorally identical** to the original — several tests fail due to subtle behavioral differences.

The root cause is that the architectural decisions in the Rust port — specifically the **channel-based communication model** and the **live-state-vs-snapshots data model** — fundamentally changed the timing and ordering of interactions between the tracker and diff tracker objects. Even though the Rust code implements the same *algorithm*, the execution model is different enough that the final results diverge.

This plan details a rearchitecture of the Rust implementation to **mirror the C++ lifetime/ownership, execution, and data models exactly**, so that all subtle behavioral differences disappear and tests pass.

---

## Retrieving the Original C++ Implementation

The original C++ files (at merge base `b1c3c0e`) are accessible via git:

```bash
# MidiStateTracker (original implementation)
git show b1c3c0e:src/backend/internal/MidiStateTracker.cpp
git show b1c3c0e:src/backend/internal/MidiStateTracker.h

# MidiStateDiffTracker (original implementation)
git show b1c3c0e:src/backend/internal/MidiStateDiffTracker.cpp
git show b1c3c0e:src/backend/internal/MidiStateDiffTracker.h

# MidiStateTracker (original header, saved as reference)
git show b1c3c0e:src/backend/internal/MidiStateTracker.h > src/backend/internal/MidiStateTracker.original.h
```

---

## Current State of the Port

The current Rust code lives under `src/rust/backend_rust/src/`:

| File | Purpose |
|------|---------|
| `midi_state_tracker.rs` | Rust `MidiStateTracker` implementation |
| `midi_state_diff_tracker.rs` | Rust `MidiStateDiffTracker` implementation |
| `event.rs` | Event types for channel-based communication |
| `midi_helpers.rs` | MIDI message parsing utilities |
| `cxx.rs` | CXX FFI bridge definitions |
| `lib.rs` | Crate root |

The C++ wrappers (`src/backend/internal/MidiStateTracker.cpp`, `src/backend/internal/MidiStateDiffTracker.cpp`) delegate to Rust objects stored in `rust::Box<backend_rust::MidiStateTracker>` etc. These wrappers are mostly thin and should remain largely unchanged — the rearchitecture is inside the Rust crate.

---

## Build and Test Commands

```bash
# Build the Rust crate (produces static library linked into the C++ backend)
cargo build -p backend_rust

# Or with features:
cargo build -p backend_rust --features prebuild

# Run the full test suite (C++ test runner + Rust unit tests)
cargo test -p backend_rust

# Run only the MIDI-related tests (via CTest)
cd build && ctest -R Midi --output-on-failure

# Run a specific test case
cd build && ./backend_test -- "[AudioMidiLoop][midi]"
```

---

## Problems with the Current Architecture

### Problem 1: Channel-Based Communication (Channels vs Callbacks)

The current Rust design uses `crossbeam_channel` for tracker → diff tracker communication:

- `MidiStateTracker` has `subscribers: Vec<Sender<Event>>`
- When a message is processed, `notify()` calls `sender.try_send(event)` — non-blocking, can drop events if the channel is full
- `MidiStateDiffTracker` must **explicitly call `sync()`** to drain events from the channel

**C++ does it differently:**
- `MidiStateTracker` holds `std::set<shoop_weak_ptr<Subscriber>>` and calls `ss->note_changed(...)` **synchronously inline** during `process_noteOn`
- Every state change immediately updates the diff tracker's diff set — no buffering, no deferral, no overflow possible (as long as the weak_ptr is valid)

**Behavioral impact:**
- Events can be silently dropped in Rust (`try_send` fails → event lost)
- The diff tracker's diff set is **stale** until `sync()` is called — any code path that calls `check_note()`, `check_cc()` etc. without first calling `sync()` reads live state that may not reflect pending events
- The `reset()` method creates **new** subscriptions each time but never removes old ones — a memory leak where every reset adds more senders to the tracker's subscriber list, causing redundant work and potential channel overflow

### Problem 2: Live State vs Snapshots (Data Model)

**C++ data model:**
When the diff tracker's `note_changed()` callback is invoked, it reads the **other tracker's live state** at the moment of the callback:
```cpp
other->maybe_current_note_velocity(channel, note) != maybe_velocity
```
Here `maybe_velocity` is the event's value (sender's new state), and `other->maybe_current_note_velocity` is the other tracker's current state. The comparison happens **at callback time**, which is immediately after the sender updated its state but before `process_noteOn` returns.

**Rust data model (current):**
The Rust code reads live state from **both** trackers at `sync()` time, **ignoring** the event's embedded values:
```rust
let sender_vel = self.with_tracker_state(sender_tracker, |t| {
    t.maybe_current_note_velocity(channel, note)  // reads live, event.param2 IGNORED
});
let other_vel = self.with_tracker_state(&other_tracker, |t| {
    t.maybe_current_note_velocity(channel, note)
});
if sender_vel != other_vel {
    self.diffs.insert([0x90 | channel, note]);
}
```
The event is essentially just a signal saying "something changed on this tracker" — the actual values come from live reads at sync time.

**Behavioral impact:**
- In the normal case (sync immediately after processing), these are equivalent. But if multiple messages are processed before sync, the live reads see the **final state** rather than the **intermediate states** at each event time.
- The `check_note()`, `check_cc()`, etc. methods read live state **without draining pending events**. If there are queued events that should affect the diff, but `check_*()` is called without `sync()`, the diff set may be updated based on live state while pending events sit in the channel — leading to inconsistencies.
- The `Rc<RefCell<*mut T>>` pattern for holding C++ tracker pointers is not tracked by Rust's borrow checker — it's unsafe and relies on programmer discipline to ensure the C++ objects outlive the Rust references.

---

## The Plan

We will rearchitect the Rust implementation to **mirror the C++ lifetime/ownership, execution, and data models exactly**. The goal is behavioral equivalence: every operation in the Rust code should produce the same result as the equivalent operation in the C++ code, in the same order, with the same guarantees.

### Design Principles (matching C++ exactly)

1. **Synchronous trait-based callbacks** — no channels, no buffering, no async
2. **Weak subscription via raw trait object pointers** — tracker holds `*mut dyn Subscriber`; diff tracker implements the trait directly
3. **Snapshot-style diff decisions** — diff tracker reads both tracker states at callback invocation time, not at sync time
4. **Proper ownership tracking** — wrap C++ tracker pointers in `Rc<RefCell<Option<Box<T>>>>` so Rust can hold strong references while the C++ side owns the objects

### Why This Is Simpler Than Expected

**Critical insight:** `MidiStateDiffTracker` is the **only subscriber** to `MidiStateTracker` in the entire codebase. There are no other C++ objects using the `MidiStateTracker::Subscriber` interface, and no plans for future subscribers. Both the tracker and diff tracker live in Rust (via FFI from C++ wrappers).

This means the subscription mechanism can be **entirely internal to Rust**. There is no need to route callbacks through the FFI boundary, no need for `Box<dyn FnMut>` closures, no need for `WeakTrackerRef` self-referential gymnastics, and no need for complex FFI changes to support subscription.

Instead, we use a **Rust trait** `Subscriber` that `MidiStateDiffTracker` implements directly. The tracker stores raw `*mut dyn Subscriber` pointers. When a state changes, the tracker calls the trait method directly. This is:
- Simpler than closures (no capture machinery)
- Simpler than FFI callbacks (no `Box<dyn FnMut>` through cxx)
- Matches C++ virtual dispatch exactly (`ss->note_changed(...)`)
- Zero overhead compared to the C++ virtual call

---

## New Architecture for `MidiStateTracker`

### The `Subscriber` Trait

```rust
/// Trait for objects that receive synchronous state change notifications.
/// This is the Rust equivalent of C++ `MidiStateTracker::Subscriber`.
pub trait Subscriber {
    fn note_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        note: u8,
        maybe_velocity: Option<u8>,
    );
    fn cc_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        cc: u8,
        maybe_value: Option<u8>,
    );
    fn program_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        maybe_program: Option<u8>,
    );
    fn pitch_wheel_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        maybe_pitch: Option<u16>,
    );
    fn channel_pressure_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        maybe_pressure: Option<u8>,
    );
}
```

**Matching C++:**
- C++ has a pure virtual class `MidiStateTracker::Subscriber` with 5 methods
- Rust has a trait `Subscriber` with 5 methods — identical semantics
- The `from: &MidiStateTracker` parameter corresponds to C++'s `MidiStateTracker *from` — the sender tracker
- `Option<u8>` corresponds to C++ `std::optional<uint8_t>`

### Data Structure

```rust
pub struct MidiStateTracker {
    id: u64,
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    
    // State — owned, mutable
    notes_active: Vec<u8>,
    controls: Vec<u8>,
    programs: Vec<u8>,
    pitch_wheel: Vec<u16>,
    channel_pressure: Vec<u8>,
    n_notes_active: std::sync::atomic::AtomicI32,
    
    // Subscribers — interior mutability via RefCell
    // C++: std::set<shoop_weak_ptr<Subscriber>> with lock() to upgrade
    // Rust: Vec<*mut dyn Subscriber> — direct trait object pointers
    subscribers: RefCell<Vec<*mut dyn Subscriber>>,
}
```

**Key decisions:**
- `subscribers: RefCell<Vec<*mut dyn Subscriber>>` enables `subscribe()`/`unsubscribe()` without requiring `&mut MidiStateTracker` (matching C++ where `subscribe()` is not const but doesn't invalidate other subscribers)
- `*mut dyn Subscriber` is a fat pointer (data ptr + vtable ptr) — valid in Rust, zero-cost, and matches C++ vtable dispatch
- No `Rc`, `Weak`, or closures — the subscriber manages its own lifetime via explicit `unsubscribe()` calls (matching C++ `shared_from_this()` pattern where the caller must explicitly unsubscribe)

### Subscription Lifecycle

**Subscribe:**
```rust
impl MidiStateTracker {
    /// Called from MidiStateDiffTracker::reset() after wrapping new trackers.
    /// subscriber: raw pointer to the diff tracker (which implements Subscriber)
    /// 
    /// C++ equivalent: m_subscribers.insert(s) where s is a shoop_weak_ptr<Subscriber>
    /// Rust: store the raw trait object pointer directly
    pub fn subscribe(&self, subscriber: *mut dyn Subscriber) {
        self.subscribers.borrow_mut().push(subscriber);
    }
    
    /// Called from MidiStateDiffTracker::reset() before changing trackers.
    /// Removes the subscriber by pointer identity.
    /// 
    /// C++ equivalent: m_subscribers.erase(s)
    pub fn unsubscribe(&mut self, subscriber: *mut dyn Subscriber) {
        self.subscribers.borrow_mut().retain(|&s| s != subscriber);
    }
}
```

**Matching C++:**
- C++ `subscribe()` adds to `std::set<shoop_weak_ptr<Subscriber>>` — keyed by `shared_ptr` identity
- C++ `unsubscribe()` removes from the set by iterating and calling `erase(s)` — keyed by `shared_ptr` identity
- Rust uses raw pointer identity (`s != subscriber`) for the same key semantics
- C++ `Subscriber` is a virtual base class; Rust `Subscriber` is a trait — both use vtable dispatch

### Callback Invocation (Synchronous, Matching C++)

```rust
impl MidiStateTracker {
    fn notify_note_changed(&self, channel: u8, note: u8, velocity: u8) {
        for &subscriber in self.subscribers.borrow().iter() {
            // SAFETY: subscribers are always valid pointers because:
            // 1. reset() unsubscribes old before subscribing new
            // 2. Drop on diff tracker unsubscribes before destruction
            // 3. The only subscriber is MidiStateDiffTracker, which C++ owns
            unsafe {
                (*subscriber).note_changed(self, channel, note, Some(velocity));
            }
        }
    }
    
    fn notify_cc_changed(&self, channel: u8, cc: u8, value: u8) {
        for &subscriber in self.subscribers.borrow().iter() {
            unsafe {
                (*subscriber).cc_changed(self, channel, cc, Some(value));
            }
        }
    }
    
    fn notify_program_changed(&self, channel: u8, program: u8) {
        for &subscriber in self.subscribers.borrow().iter() {
            unsafe {
                (*subscriber).program_changed(self, channel, Some(program));
            }
        }
    }
    
    fn notify_pitch_wheel_changed(&self, channel: u8, pitch: u16) {
        for &subscriber in self.subscribers.borrow().iter() {
            unsafe {
                (*subscriber).pitch_wheel_changed(self, channel, Some(pitch));
            }
        }
    }
    
    fn notify_channel_pressure_changed(&self, channel: u8, pressure: u8) {
        for &subscriber in self.subscribers.borrow().iter() {
            unsafe {
                (*subscriber).channel_pressure_changed(self, channel, Some(pressure));
            }
        }
    }
}
```

**Matching C++:**
- C++ iterates `m_subscribers`, calls `if (auto ss = s.lock()) { ss->note_changed(...) }` — lock upgrade determines whether callback fires
- Rust uses `(*subscriber).note_changed(...)` — the pointer is always valid because the diff tracker manages its own lifetime via explicit unsubscribe in `reset()` and `Drop`
- The `unsafe` block is sound because the subscriber list is maintained by explicit subscribe/unsubscribe calls that mirror the C++ lifecycle exactly

---

## New Architecture for `MidiStateDiffTracker`

### TrackerRef: Wrapping C++ Raw Pointers

```rust
/// Wraps a raw pointer from C++ (Box<MidiStateTracker> owned by C++) in a
/// reference-counted handle so we can hold strong references in Rust.
/// The inner Option<Box<...>> lets us set None during reset() to "deactivate"
/// the handle without deallocating (matching C++ nullptr behavior).
struct TrackerRef(Rc<RefCell<Option<Box<MidiStateTracker>>>>);

impl TrackerRef {
    /// Wrap a raw pointer. Called during reset() with the C++ tracker's raw ptr.
    /// SAFETY: tracker must be a valid Box<MidiStateTracker> owned by C++,
    /// and must outlive this TrackerRef for the lifetime of this diff tracker.
    /// The C++ object lifecycle guarantees this (C++ MidiStateDiffTracker
    /// calls reset() and the C++ tracker's lifetime is at least the C++ diff tracker's).
    unsafe fn new(tracker: *mut MidiStateTracker) -> Self {
        Self(Rc::new(RefCell::new(Some(Box::from_raw(tracker)))))
    }
    
    fn with_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&MidiStateTracker) -> R,
    {
        let inner = self.0.borrow();
        let tracker = inner.as_ref().ok().unwrap();
        // SAFETY: tracker is valid for the lifetime of this TrackerRef (C++ ownership)
        f(unsafe { tracker.as_ref().unwrap() })
    }
}
```

**Note: This pattern requires `std::cell::RefCell` — not `std::sync::Mutex` — because the entire MIDI processing runs on a single thread.** The C++ code is not thread-safe and makes no thread safety guarantees. Using `Mutex` would introduce unnecessary synchronization overhead and could deadlock if callbacks try to acquire locks held by the owning code. The single-threaded model means interior mutability via `RefCell` is safe and appropriate.

### Data Structure

```rust
pub struct MidiStateDiffTracker {
    id: u64,
    
    // Live references to trackers (strong Rc, keep alive while held)
    // C++: shoop_shared_ptr<MidiStateTracker> m_a, m_b
    tracker_a: Option<TrackerRef>,
    tracker_a_id: u64,
    tracker_b: Option<TrackerRef>,
    tracker_b_id: u64,
    
    // Diff state
    // C++: boost::container::flat_set<DifferingState>
    // Rust: BTreeSet (ordered, same complexity guarantees)
    diffs: std::collections::BTreeSet<[u8; 2]>,
}
```

**Matching C++:**
- C++ `reset()` calls `m_a->unsubscribe(shared_from_this())` then `m_a->subscribe(shared_from_this())` — unsubscribe before reset, subscribe after
- Rust `reset()` must do the same: unsubscribe old trackers, then set new tracker refs, then subscribe new trackers

### Implementing the `Subscriber` Trait

```rust
impl Subscriber for MidiStateDiffTracker {
    fn note_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        note: u8,
        maybe_velocity: Option<u8>,
    ) {
        // Determine which tracker sent the event and which is the other
        let (sender_ref, other_ref) = if from.get_id() == self.tracker_a_id {
            (&self.tracker_a, &self.tracker_b)
        } else {
            (&self.tracker_b, &self.tracker_a)
        };
        
        let sender_ref = match sender_ref.as_ref() {
            Some(r) => r,
            None => return,
        };
        let other_ref = match other_ref.as_ref() {
            Some(r) => r,
            None => return,
        };
        
        // Read LIVE state of both trackers at callback time — matches C++ exactly
        let sender_vel = sender_ref.with_state(|t| t.maybe_current_note_velocity(channel, note));
        let other_vel  = other_ref.with_state(|t| t.maybe_current_note_velocity(channel, note));
        
        // C++: if (other->maybe_current_note_velocity(channel, note) != maybe_velocity)
        // where maybe_velocity is the event's value (sender's new state)
        // Rust: sender_vel != other_vel (live comparison)
        // These are equivalent when called synchronously during process_msg
        if sender_vel != other_vel {
            self.diffs.insert([0x90 | channel, note]);
        } else {
            self.diffs.remove(&[0x90 | channel, note]);
        }
    }
    
    fn cc_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        cc: u8,
        maybe_value: Option<u8>,
    ) {
        let (sender_ref, other_ref) = if from.get_id() == self.tracker_a_id {
            (&self.tracker_a, &self.tracker_b)
        } else {
            (&self.tracker_b, &self.tracker_a)
        };
        
        let sender_ref = match sender_ref.as_ref() { Some(r) => r, None => return };
        let other_ref = match other_ref.as_ref() { Some(r) => r, None => return };
        
        let sender_val = sender_ref.with_state(|t| t.maybe_current_cc_value(channel, cc));
        let other_val  = other_ref.with_state(|t| t.maybe_current_cc_value(channel, cc));
        
        if sender_val != other_val {
            self.diffs.insert([0xB0 | channel, cc]);
        } else {
            self.diffs.remove(&[0xB0 | channel, cc]);
        }
    }
    
    fn program_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        maybe_program: Option<u8>,
    ) {
        // ... same pattern: compare sender's live state to other's live state ...
    }
    
    fn pitch_wheel_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        maybe_pitch: Option<u16>,
    ) {
        // ... same pattern ...
    }
    
    fn channel_pressure_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        maybe_pressure: Option<u8>,
    ) {
        // ... same pattern ...
    }
}
```

**Matching C++:**
- C++ `note_changed` reads `other->maybe_current_note_velocity(channel, note)` and compares to `maybe_velocity`
- Rust reads both tracker's live state and compares — equivalent because the callback fires immediately after the sender updates its state
- The `from.get_id() == self.tracker_a_id` check is the Rust equivalent of C++'s `if (tracker != m_a.get() && tracker != m_b.get()) { return; }` — identifying the sender tracker

### The `reset()` Method

```rust
impl MidiStateDiffTracker {
    /// Called from C++ wrapper: MidiStateDiffTracker::reset(a, b, action)
    /// 
    /// C++ order of operations:
    ///   1. if (m_a) m_a->unsubscribe(shared_from_this())
    ///   2. if (m_b) m_b->unsubscribe(shared_from_this())
    ///   3. m_a = a, m_b = b
    ///   4. if (m_a) m_a->subscribe(shared_from_this())
    ///   5. if (m_b) m_b->subscribe(shared_from_this())
    ///   6. [switch action: rescan_diff() or clear_diff()]
    /// 
    /// We must match this exactly.
    pub unsafe fn reset(
        &mut self,
        a: *mut MidiStateTracker,
        b: *mut MidiStateTracker,
        action: i32,
    ) {
        // Step 1-2: Unsubscribe from old trackers
        if let Some(ref ta) = self.tracker_a {
            ta.with_state(|t| t.unsubscribe(self as *mut dyn Subscriber));
        }
        if let Some(ref tb) = self.tracker_b {
            tb.with_state(|t| t.unsubscribe(self as *mut dyn Subscriber));
        }
        
        // Step 3: Set new tracker refs (wrap raw pointers)
        self.tracker_a = if a.is_null() { None } else { Some(TrackerRef::new(a)) };
        self.tracker_b = if b.is_null() { None } else { Some(TrackerRef::new(b)) };
        
        // Get tracker IDs (needed in callbacks to identify sender)
        self.tracker_a_id = self.tracker_a.as_ref()
            .map(|r| r.with_state(|t| t.get_id()))
            .unwrap_or(0);
        self.tracker_b_id = self.tracker_b.as_ref()
            .map(|r| r.with_state(|t| t.get_id()))
            .unwrap_or(0);
        
        // Steps 4-5: Subscribe to new trackers
        if let Some(ref ta) = self.tracker_a {
            ta.with_state(|t| t.subscribe(self as *mut dyn Subscriber));
        }
        if let Some(ref tb) = self.tracker_b {
            tb.with_state(|t| t.subscribe(self as *mut dyn Subscriber));
        }
        
        // Step 6: Action
        match action {
            0 => self.rescan_diff(),   // StateDiffTrackerAction::ScanDiff
            1 => self.clear_diff(),    // StateDiffTrackerAction::ClearDiff
            _ => {}
        }
    }
}
```

### Drop Handler

```rust
impl Drop for MidiStateDiffTracker {
    fn drop(&mut self) {
        // Unsubscribe from trackers before destruction
        // This is the Rust equivalent of C++ ~MidiStateDiffTracker() calling unsubscribe
        if let Some(ref ta) = self.tracker_a {
            ta.with_state(|t| t.unsubscribe(self as *mut dyn Subscriber));
        }
        if let Some(ref tb) = self.tracker_b {
            tb.with_state(|t| t.unsubscribe(self as *mut dyn Subscriber));
        }
    }
}
```

**Safety:**
- `reset()` always unsubscribes old trackers before subscribing new ones
- `Drop` unsubscribes on destruction
- These two guarantees ensure the tracker never holds a dangling `*mut dyn Subscriber`
- The `unsafe` in `notify_*_changed` is therefore sound

---

## Remove `sync()` — Callbacks Are Synchronous

The current `sync()` method drains events from channels and processes them. In the new architecture:

- **There are no channels** — callbacks are invoked synchronously during `process_msg`
- **There is no event queue** — every state change immediately updates the diff set via callback
- **There is no `sync()`** — the diff set is always current

This means:
1. Remove `sync()` from `MidiStateDiffTracker`
2. Remove the `crossbeam-channel` dependency from `Cargo.toml`
3. Remove the `Event` type and all channel-based code from `event.rs`
4. Remove `create_subscription()` from `MidiStateTracker`

---

## Diff Resolution (No Changes Needed)

The `resolve_to()` / `resolve_to_wrapper()` methods read live state from the trackers and generate output messages. This behavior is already correct — it reads current state and produces messages to make the target match the source. The change to synchronous callbacks means the diff set is always up-to-date, so `resolve_to()` will produce the same results as the C++ version.

---

## FFI Interface Changes

### `MidiStateTracker` FFI

**No changes needed for subscription.** `subscribe()` and `unsubscribe()` are now pure Rust methods — they are only called from `MidiStateDiffTracker::reset()` and `Drop`, which are both inside Rust.

The cxx bridge in `cxx.rs` keeps the existing interface:

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiStateTracker;
        
        // Existing methods remain unchanged:
        fn new_midi_state_tracker(...) -> Box<MidiStateTracker>;
        unsafe fn process_msg_raw(self: &mut MidiStateTracker, data: *const u8);
        // ... accessors, copy_relevant_state, clear, etc.
    }
}
```

The `subscribe`/`unsubscribe` methods are **not exposed through FFI** — they are internal to Rust.

### `MidiStateDiffTracker` FFI

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiStateDiffTracker;
        
        // reset takes action enum as i32 (or use cxx's enum support)
        unsafe fn reset(
            self: &mut MidiStateDiffTracker,
            a: *mut MidiStateTracker,
            b: *mut MidiStateTracker,
            action: i32,
        );
        
        // Existing methods remain unchanged:
        // add_diff, delete_diff, check_note, check_cc, check_program,
        // check_channel_pressure, check_pitch_wheel, rescan_diff, clear_diff,
        // resolve_to_wrapper, get_diff_flat, set_diff_flat, get_id
    }
}
```

**No new FFI methods needed.** The subscription mechanism is entirely internal to Rust.

---

## C++ Wrapper Changes (Minimal)

The C++ wrappers (`src/backend/internal/MidiStateTracker.cpp`, `src/backend/internal/MidiStateDiffTracker.cpp`) are mostly already thin delegates. **No changes needed** for the subscription mechanism:

### `MidiStateTracker.cpp`

```cpp
void MidiStateTracker::subscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    // NO-OP: subscription is handled entirely inside Rust
    // The Rust MidiStateDiffTracker::reset() calls tracker->subscribe() directly
}

void MidiStateTracker::unsubscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    // NO-OP: unsubscription is handled entirely inside Rust
    // The Rust MidiStateDiffTracker::reset() and Drop call tracker->unsubscribe() directly
}
```

These can remain no-ops. The C++ wrapper doesn't need to do anything because the Rust diff tracker manages its own subscription lifecycle.

### `MidiStateDiffTracker.cpp`

The `reset()` method already calls the Rust implementation:

```cpp
void MidiStateDiffTracker::reset(
    shoop_shared_ptr<MidiStateTracker> a,
    shoop_shared_ptr<MidiStateTracker> b,
    StateDiffTrackerAction action
) {
    m_a = a;
    m_b = b;
    rust::Box<backend_rust::MidiStateDiffTracker> &r = ...;
    r->reset(
        a ? a->get_rust().get() : nullptr,
        b ? b->get_rust().get() : nullptr,
        static_cast<int>(action)
    );
}
```

This works as-is. The Rust `reset()` now manages subscriptions internally.

---

## Implementation Sequence

1. **Update `Cargo.toml`** — remove `crossbeam-channel` dependency
2. **Update `cxx.rs`** — no changes needed for subscription (it stays internal)
3. **Rewrite `midi_state_tracker.rs`**:
   - Add the `Subscriber` trait with 5 methods
   - Replace `subscribers: Vec<Sender<Event>>` with `subscribers: RefCell<Vec<*mut dyn Subscriber>>`
   - Implement `subscribe()` and `unsubscribe()` (pure Rust)
   - Replace channel-based `notify()` with direct trait method calls via `notify_note_changed`, `notify_cc_changed`, etc.
   - Remove `create_subscription()` and all channel-related code
4. **Rewrite `midi_state_diff_tracker.rs`**:
   - Implement `Subscriber` for `MidiStateDiffTracker`
   - Replace `subscriptions: Vec<Subscription>` with `TrackerRef` fields
   - Implement `TrackerRef` wrapper type
   - Implement `reset()` with proper subscribe/unsubscribe lifecycle
   - Implement `Drop` to unsubscribe on destruction
   - Remove `sync()` entirely
   - Remove all channel-based code
5. **Rewrite `event.rs`** — remove `Event` type, `TrackerStateData`, and channel types. This file may become minimal or be removed entirely.
6. **Build and test** — verify compilation, then run test suite to confirm behavioral equivalence

---

## Why This Eliminates All Behavioral Differences

| Aspect | Before (Channels) | After (Trait Callbacks) | C++ Behavior |
|--------|-------------------|------------------------|-------------|
| **Event delivery** | `try_send` → events dropped if channel full | Direct trait method call → no drops | `ss->note_changed(...)` called inline, no drops |
| **Callback timing** | Deferred via `sync()` (async, buffered) | Synchronous during `process_msg` | Inline during `process_noteOn` |
| **Diff decision data** | Reads live state at `sync()` time, ignores event values | Reads live state at callback time (both trackers) | Reads other tracker live state vs event value at callback time |
| **Subscription lifecycle** | No-ops; `reset()` accumulated senders, never removed | `subscribe()`/`unsubscribe()` manage raw pointers; `reset()` unsubscribes old before subscribing new; `Drop` cleans up | `unsubscribe()` then `subscribe()` in `reset()`; destructor unsubscribes |
| **Memory safety** | `Rc<RefCell<*mut T>>` not tracked by borrow checker | `*mut dyn Subscriber` with explicit unsubscribe in `reset()` and `Drop` | `weak_ptr` for subscriber, explicit `shared_from_this()` in destructor |
| **Channel overflow** | `try_send` drops events silently | N/A (no channels) | N/A (no channels) |
| **FFI complexity** | Would need 5 callback types through cxx | Zero FFI changes for subscription | N/A (pure C++) |

---

## Verification

After implementation, the following should hold:

1. **Compilation**: `cargo build -p backend_rust` succeeds, no warnings
2. **Tests pass**: `cargo test -p backend_rust` and `ctest -R Midi` all pass
3. **Event ordering**: For any sequence of `process_msg` calls, the diff set is identical to the C++ version (verified by running the same message sequence through both implementations and comparing diffs)
4. **No memory leaks**: Subscriptions are properly cleaned up on `reset()` and destruction — verified by checking that the tracker's subscriber list does not grow unboundedly across repeated reset cycles
5. **No dangling pointers**: The `unsafe` blocks in `notify_*_changed` are sound because explicit unsubscribe in `reset()` and `Drop` guarantees no subscriber outlives the diff tracker
