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

1. **Callback-based synchronous communication** — no channels, no buffering, no async
2. **Weak subscription lifecycle** — tracker holds `Rc<RefCell<Option<Box<dyn FnMut>>>>` for callbacks; diff tracker holds weak references
3. **Snapshot-style diff decisions** — diff tracker reads both tracker states at callback invocation time, not at sync time
4. **Proper ownership tracking** — wrap C++ tracker pointers in `Rc<RefCell<Option<Box<T>>>>` so Rust can hand out weak handles and properly manage lifetimes

---

## New Architecture for `MidiStateTracker`

### Data Structure

```rust
use std::rc::{Rc, Weak};
use std::cell::RefCell;

struct SubscriberSlot {
    // The callback closure, wrapped so it can be invalidated on unsubscribe
    callback: Rc<RefCell<Option<Box<dyn FnMut(u8, u8, u8)>>>>,
    // Pointer identity key for O(n) unsubscribe lookup
    key: *const c_void,
}

pub struct MidiStateTracker {
    id: u64,
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    
    // State — owned, mutable (RefCell for interior mutability of subscribers list only)
    notes_active: Vec<u8>,
    controls: Vec<u8>,
    programs: Vec<u8>,
    pitch_wheel: Vec<u16>,
    channel_pressure: Vec<u8>,
    n_notes_active: std::sync::atomic::AtomicI32,
    
    // Subscribers — interior mutability via RefCell
    // C++: std::set<shoop_weak_ptr<Subscriber>> with lock() to upgrade
    // Rust: Vec<SubscriberSlot> with try_borrow_mut() to invoke
    subscribers: RefCell<Vec<SubscriberSlot>>,
}
```

**Key decisions:**
- `subscribers: RefCell<Vec<SubscriberSlot>>` enables `subscribe()`/`unsubscribe()` without requiring `&mut MidiStateTracker` (matching C++ where `subscribe()` is not const but doesn't invalidate other subscribers)
- Each slot's callback is `Rc<RefCell<Option<Box<dyn FnMut>>>>` — the `Rc` pointer identity is used for `unsubscribe()` lookup (matching C++ pointer-based keying in `std::set`)
- The `RefCell<Option<...>>` allows the callback to be temporarily taken during invocation (matching C++ `if (auto ss = s.lock())` pattern where the lock upgrade determines whether the callback fires)

### Subscription Lifecycle

**Subscribe:**
```rust
impl MidiStateTracker {
    /// C++ calls this from MidiStateDiffTracker::reset() after wrapping the diff tracker.
    /// subscriber_ptr: the C++ MidiStateDiffTracker 'this' pointer (used as key for unsubscribe)
    /// callback: Box<dyn FnMut(channel, note, velocity)> — the diff tracker's callback closure
    /// Returns: handle (the Rc pointer) that C++ stores for later unsubscribe
    pub fn subscribe(
        &self,
        subscriber_ptr: *mut c_void,
        callback: Box<dyn FnMut(u8, u8, u8)>,
    ) -> *mut c_void {
        let callback = Rc::new(RefCell::new(Some(callback)));
        let key = subscriber_ptr;
        
        self.subscribers.borrow_mut().push(SubscriberSlot {
            callback: Rc::clone(&callback),
            key,
        });
        
        // Return raw pointer to the Rc — used as handle in unsubscribe
        Rc::into_raw(callback) as *mut c_void
    }
    
    /// C++ calls this from MidiStateDiffTracker::reset() before changing trackers.
    /// Removes the slot whose handle matches.
    pub fn unsubscribe(&mut self, handle: *mut c_void) {
        let handle_rc = handle as *const Rc<RefCell<Option<Box<dyn FnMut(u8, u8, u8)>>>>;
        self.subscribers.borrow_mut().retain(|slot| {
            Rc::as_ptr(&slot.callback) != handle_rc
        });
    }
}
```

**Matching C++:**
- C++ `subscribe()` adds to `std::set<shoop_weak_ptr<Subscriber>>` — keyed by `shared_ptr` identity
- C++ `unsubscribe()` removes from the set by iterating and calling `erase(s)` — keyed by `shared_ptr` identity
- Rust uses `Rc::as_ptr()` for the same key semantics

### Callback Invocation (Synchronous, Matching C++)

```rust
impl MidiStateTracker {
    fn notify_note_changed(&self, channel: u8, note: u8, velocity: u8) {
        for slot in self.subscribers.borrow().iter() {
            if let Ok(mut cb) = slot.callback.try_borrow_mut() {
                if let Some(mut f) = cb.take() {
                    f(channel, note, velocity);
                    *cb = Some(f); // Re-wrap (callback may persist via interior mutability)
                }
            }
        }
    }
    
    fn notify_cc_changed(&self, channel: u8, cc: u8, value: u8) { /* similar */ }
    fn notify_program_changed(&self, channel: u8, program: u8) { /* similar */ }
    fn notify_pitch_wheel_changed(&self, channel: u8, pitch: u16) { /* similar */ }
    fn notify_channel_pressure_changed(&self, channel: u8, pressure: u8) { /* similar */ }
}
```

**Matching C++:**
- C++ iterates `m_subscribers`, calls `if (auto ss = s.lock()) { ss->note_changed(...) }` — lock upgrade determines whether callback fires
- Rust uses `try_borrow_mut()` to get exclusive access to each callback — failure means another borrow is in progress (unlikely in single-threaded context, but matches C++ "skip if can't upgrade")

---

## New Architecture for `MidiStateDiffTracker`

### TrackerRef: Wrapping C++ Raw Pointers

```rust
/// Wraps a raw pointer from C++ (Box<MidiStateTracker> owned by C++) in a
/// reference-counted handle so we can hand out Weak references.
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

/// Weak version of TrackerRef — held by the tracker so it can invoke callbacks
/// without keeping the diff tracker alive.
struct WeakTrackerRef(Weak<RefCell<Option<Box<MidiStateTracker>>>>);

impl WeakTrackerRef {
    fn new(strong: &TrackerRef) -> Self {
        Self(Rc::downgrade(&strong.0))
    }
    
    fn upgrade(&self) -> Option<TrackerRef> {
        self.0.upgrade().map(Self)
    }
}
```

**Note: This pattern requires `std::cell::RefCell` — not `std::sync::Mutex` — because the entire MIDI processing runs on a single thread.** The C++ code is not thread-safe and makes no thread safety guarantees. Using `Mutex` would introduce unnecessary synchronization overhead and could deadlock if callbacks try to acquire locks held by the owning code. The single-threaded model means interior mutability via `RefCell` is safe and appropriate.

**Circular reference safety**: The diff tracker wraps itself in `TrackerRef` to hand out `WeakTrackerRef` to the tracker. The tracker holds the callback closure (which captures `WeakTrackerRef`). This creates the following reference chain: diff tracker → `TrackerRef` (strong) → tracker → callback closure → `WeakTrackerRef` → diff tracker (weak). Since the tracker's callback holds a `Weak` (not a strong `Rc`) to the diff tracker, the cycle is broken — when the diff tracker is dropped, its `Rc` strong count goes to zero, and the `Weak` upgrades fail, causing callbacks to silently no-op. This matches C++ `weak_ptr` behavior exactly.

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
    
    // Weak handle to our own callback slot (given to trackers for invocation)
    // C++: the diff tracker IS the Subscriber, registered via subscribe()
    // Rust: we create a callback closure that captures Weak<Self>, upgrade at invoke time
    weak_self: RefCell<Option<WeakTrackerRef>>,
    
    // The handle returned by subscribe() — stored so unsubscribe can find it
    // C++: unsubscribe called with shared_from_this() identity
    subscribe_handle: RefCell<Option<*mut c_void>>,
    
    // Diff state
    // C++: boost::container::flat_set<DifferingState>
    // Rust: BTreeSet (ordered, same complexity guarantees)
    diffs: std::collections::BTreeSet<[u8; 2]>,
}
```

**Matching C++:**
- C++ `reset()` calls `m_a->unsubscribe(shared_from_this())` then `m_a->subscribe(shared_from_this())` — unsubscribe before reset, subscribe after
- Rust `reset()` must do the same: unsubscribe old trackers, then set new tracker refs, then subscribe new trackers

### The Callback Closure

When `MidiStateDiffTracker::reset()` is called, it:
1. Unsubscribes from old trackers using the stored `subscribe_handle`
2. Wraps new tracker raw pointers in `TrackerRef`
3. Creates a callback closure for each tracker that captures a `Weak<Self>` reference
4. Calls `tracker->subscribe(self_ptr, callback)` to register

The callback closure pattern:
```rust
fn make_note_callback(
    tracker_id: u64,
    weak_self: WeakTrackerRef,
) -> Box<dyn FnMut(u8, u8, u8)> {
    Box::new(move |channel: u8, note: u8, velocity: u8| {
        // Upgrade weak to get &mut MidiStateDiffTracker
        let strong = match weak_self.upgrade() {
            Some(s) => s,
            None => return, // Diff tracker gone — matching C++ "ignore if tracker not known"
        };
        
        let mut diff_tracker = strong.0.borrow_mut();
        let this = diff_tracker.as_mut().unwrap();
        
        // Determine sender and other trackers
        let (sender_ref, other_ref) = if tracker_id == this.tracker_a_id {
            (&this.tracker_a, &this.tracker_b)
        } else {
            (&this.tracker_b, &this.tracker_a)
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
            this.diffs.insert([0x90 | channel, note]);
        } else {
            this.diffs.remove(&[0x90 | channel, note]);
        }
    })
}
```

The same pattern applies for CC, program, pitch wheel, and channel pressure callbacks — each captures `tracker_id`, `weak_self`, and reads live state from both trackers at invocation time.

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
        action: StateDiffTrackerAction,
    ) {
        // Step 1-2: Unsubscribe from old trackers
        if let Some(ref ta) = self.tracker_a {
            if let Some(handle) = self.subscribe_handle.borrow().copied() {
                ta.with_state(|t| t.unsubscribe(handle));
            }
        }
        if let Some(ref tb) = self.tracker_b {
            if let Some(handle) = self.subscribe_handle.borrow().copied() {
                tb.with_state(|t| t.unsubscribe(handle));
            }
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
        
        // Create weak ref to ourselves for callbacks to upgrade
        // TrackerRef wraps ourselves so we can hand out a Weak reference
        let self_ref = TrackerRef::new(self as *mut MidiStateDiffTracker as *mut MidiStateDiffTracker);
        let weak = WeakTrackerRef::new(&self_ref);
        
        // Steps 4-5: Subscribe to new trackers
        let handle: Option<*mut c_void> = if let Some(ref ta) = self.tracker_a {
            let cb = make_note_callback(self.tracker_a_id, WeakTrackerRef::new(ta));
            let h = ta.with_state(|t| t.subscribe_note(self as *const _ as *mut c_void, cb));
            Some(h)
        } else { None };
        
        if let Some(ref tb) = self.tracker_b {
            let cb = make_note_callback(self.tracker_b_id, WeakTrackerRef::new(tb));
            tb.with_state(|t| t.subscribe_note(self as *const _ as *mut c_void, cb));
        }
        
        *self.subscribe_handle.borrow_mut() = handle; as *const _ as *mut c_void, cb));
            Some(h)
        } else { None };
        
        if let Some(ref tb) = self.tracker_b {
            let cb = make_note_callback(self.tracker_b_id, WeakTrackerRef::new(tb));
            tb.with_state(|t| t.subscribe(self as *const _ as *mut c_void, cb));
        }
        
        *self.subscribe_handle.borrow_mut() = handle;
        
        // Step 6: Action
        match action {
            StateDiffTrackerAction::ScanDiff => self.rescan_diff(),
            StateDiffTrackerAction::ClearDiff => self.clear_diff(),
            _ => {}
        }
    }
}
```

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
4. Remove `create_subscription()` from `MidiStateTracker` (it becomes `subscribe()`)

---

## Diff Resolution (No Changes Needed)

The `resolve_to()` / `resolve_to_wrapper()` methods read live state from the trackers and generate output messages. This behavior is already correct — it reads current state and produces messages to make the target match the source. The change to synchronous callbacks means the diff set is always up-to-date, so `resolve_to()` will produce the same results as the C++ version.

---

## FFI Interface Changes

### `MidiStateTracker` FFI

The cxx bridge in `cxx.rs` needs these changes:

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiStateTracker;
        
        // NEW: subscribe/unsubscribe (replace channel-based create_subscription)
        fn subscribe(
            self: &MidiStateTracker,
            subscriber_ptr: *mut c_void,
            callback: Box<dyn FnMut(u8, u8, u8)>,
        ) -> *mut c_void;
        fn unsubscribe(self: &mut MidiStateTracker, handle: *mut c_void);
        
        // Existing methods remain
        fn new_midi_state_tracker(...) -> Box<MidiStateTracker>;
        unsafe fn process_msg_raw(self: &mut MidiStateTracker, data: *const u8);
        // ... accessors, copy_relevant_state, clear, etc.
    }
}
```

The callback signature `Box<dyn FnMut(u8, u8, u8)>` with 3 u8 args covers all event types (channel, param1, param2). For note events: (channel, note, velocity). For CC: (channel, cc, value). For program: (channel, program, 0). For pitch: (channel, lsb, msb). For pressure: (channel, pressure, 0).

**Important: C++ has 5 distinct callback methods** on the `Subscriber` interface:
- `note_changed(MidiStateTracker *from, uint8_t channel, uint8_t note, std::optional<uint8_t> maybe_velocity)`
- `cc_changed(MidiStateTracker *from, uint8_t channel, uint8_t cc, std::optional<uint8_t> value)`
- `program_changed(MidiStateTracker *from, uint8_t channel, std::optional<uint8_t> program)`
- `pitch_wheel_changed(MidiStateTracker *from, uint8_t channel, std::optional<uint16_t> pitch)`
- `channel_pressure_changed(MidiStateTracker *from, uint8_t channel, std::optional<uint8_t> pressure)`

Each method has a different signature and carries a different value type. The plan should account for registering **separate callbacks per event type**, using cxx's support for `Box<dyn Fn(...)>` where `Fn` is a concrete function pointer type.

Alternatively, use distinct callback types per event type:
```rust
fn subscribe_note(self: &MidiStateTracker, callback: Box<dyn FnMut(u8, u8, u8)>) -> *mut c_void;
fn subscribe_cc(self: &MidiStateTracker, callback: Box<dyn FnMut(u8, u8, u8)>) -> *mut c_void;
fn subscribe_program(self: &MidiStateTracker, callback: Box<dyn FnMut(u8, u8)>) -> *mut c_void;
fn subscribe_pitch(self: &MidiStateTracker, callback: Box<dyn FnMut(u8, u16)>) -> *mut c_void;
fn subscribe_pressure(self: &MidiStateTracker, callback: Box<dyn FnMut(u8, u8)>) -> *mut c_void;
```

One callback type with (u8, u8, u8) is simpler for FFI and the diff tracker can dispatch internally based on which subscription slot the callback came from. However, since C++ has 5 distinct callback methods with different signatures, the recommended approach is to register separate callbacks per event type:

```rust
fn subscribe_note(self: &MidiStateTracker, callback: Box<dyn Fn(u8, u8, u8)>) -> *mut c_void;
fn subscribe_cc(self: &MidiStateTracker, callback: Box<dyn Fn(u8, u8, u8)>) -> *mut c_void;
fn subscribe_program(self: &MidiStateTracker, callback: Box<dyn Fn(u8, u8)>) -> *mut c_void;
fn subscribe_pitch(self: &MidiStateTracker, callback: Box<dyn Fn(u8, u16)>) -> *mut c_void;
fn subscribe_pressure(self: &MidiStateTracker, callback: Box<dyn Fn(u8, u8)>) -> *mut c_void;
```

**Why `Fn` instead of `FnMut`**: cxx supports `Box<dyn Fn(...)>` but not `Box<dyn FnMut(...)>`. Use `Fn` (non-mutating) for the FFI boundary — any interior mutability needed by the diff tracker is handled via `RefCell` inside the closure's captured state. The diff tracker's callback closure captures `WeakTrackerRef` via interior mutability (`Rc<RefCell<...>>`), so it doesn't need `FnMut` at the FFI boundary.

The diff tracker creates a closure per event type that captures its internal state and `WeakTrackerRef`:

```rust
// Example: note callback closure
let cb = Box::new(move |channel: u8, note: u8, velocity: u8| {
    // Capture WeakTrackerRef via RefCell (interior mutability)
    // Upgrade and mutate diff tracker state
    let strong = weak_self.upgrade()?;
    let mut dt = strong.0.borrow_mut();
    let this = dt.as_mut().unwrap();
    // ... update diffs
});
```

### `MidiStateDiffTracker` FFI

```rust
#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiStateDiffTracker;
        
        // reset now takes action enum as i32 (or use cxx's enum support)
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

---

## C++ Wrapper Changes (Minimal)

The C++ wrappers (`src/backend/internal/MidiStateTracker.cpp`, `src/backend/internal/MidiStateDiffTracker.cpp`) are mostly already thin delegates. Minimal changes needed:

### `MidiStateTracker.cpp`

The `subscribe()` and `unsubscribe()` methods are already no-ops in the current C++ wrapper. After the rearchitecture, they become real calls:

```cpp
void MidiStateTracker::subscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    // NO-OP → call into Rust with the diff tracker's 'this' ptr and a callback
    // The diff tracker's callback captures its 'this' pointer to identify itself
    // 
    // Alternatively: the C++ caller (MidiStateDiffTracker) calls subscribe directly
    // via the FFI, so MidiStateTracker::subscribe() can be removed entirely.
    // This depends on whether subscribe is called directly from C++ code or via C++.
}

void MidiStateTracker::unsubscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    // NO-OP → call into Rust with the stored handle
    // Remove entirely if subscribe/unsubscribe are handled via direct FFI calls
}
```

### `MidiStateDiffTracker.cpp`

The `reset()` method already calls the Rust implementation. It should work as-is since the Rust `reset()` will now manage subscriptions via the new callback-based model.

---

## Implementation Sequence

1. **Update `Cargo.toml`** — remove `crossbeam-channel` dependency
2. **Update `cxx.rs`** — change FFI signatures: add `subscribe`/`unsubscribe`, remove channel-based FFI, change `reset` to take action enum
3. **Rewrite `MidiStateTracker`** (`midi_state_tracker.rs`):
   - Replace `subscribers: Vec<Sender<Event>>` with `subscribers: RefCell<Vec<SubscriberSlot>>`
   - Implement `subscribe()` and `unsubscribe()`
   - Replace channel-based `notify()` with direct callback invocation
   - Remove `create_subscription()` and all channel-related code
4. **Rewrite `MidiStateDiffTracker`** (`midi_state_diff_tracker.rs`):
   - Replace `subscriptions: Vec<Subscription>` with `TrackerRef` fields
   - Implement `TrackerRef` wrapper type
   - Implement `reset()` with proper subscribe/unsubscribe lifecycle
   - Implement callback closures that capture `WeakTrackerRef`, upgrade at invoke time, read live state
   - Remove `sync()` entirely
   - Remove all channel-based code
5. **Rewrite `event.rs`** — remove `Event` type, `TrackerStateData`, and channel types. This file may become minimal or be removed entirely.
6. **Update C++ wrappers** — make `subscribe`/`unsubscribe` real calls (or remove them if FFI is called directly from the C++ caller)
7. **Build and test** — verify compilation, then run test suite to confirm behavioral equivalence

---

## Why This Eliminates All Behavioral Differences

| Aspect | Before (Channels) | After (Callbacks + Weak Refs) | C++ Behavior |
|--------|-------------------|-------------------------------|-------------|
| **Event delivery** | `try_send` → events dropped if channel full | `FnMut` invoked directly → no drops | `ss->note_changed(...)` called inline, no drops |
| **Callback timing** | Deferred via `sync()` (async, buffered) | Synchronous during `process_msg` | Inline during `process_noteOn` |
| **Diff decision data** | Reads live state at `sync()` time, ignores event values | Reads live state at callback time (both trackers) | Reads other tracker live state vs event value at callback time |
| **Subscription lifecycle** | No-ops; `reset()` accumulated senders, never removed | `subscribe()`/`unsubscribe()` manage slots; `reset()` unsubscribes old before subscribing new | `unsubscribe()` then `subscribe()` in `reset()` |
| **Memory safety** | `Rc<RefCell<*mut T>>` not tracked by borrow checker | `Rc<RefCell<Option<Box<T>>>>` with Weak handles | `weak_ptr` for subscriber, `shared_ptr` for tracker ownership |
| **Channel overflow** | `try_send` drops events silently | N/A (no channels) | N/A (no channels) |

---

## Verification

After implementation, the following should hold:

1. **Compilation**: `cargo build -p backend_rust` succeeds, no warnings
2. **Tests pass**: `cargo test -p backend_rust` and `ctest -R Midi` all pass
3. **Event ordering**: For any sequence of `process_msg` calls, the diff set is identical to the C++ version (verified by running the same message sequence through both implementations and comparing diffs)
4. **No memory leaks**: Subscriptions are properly cleaned up on `reset()` — verified by checking that the tracker's subscriber list does not grow unboundedly across repeated reset cycles