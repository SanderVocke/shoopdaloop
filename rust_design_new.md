# Proposed New Rust MidiTracker Design

## Overview

The new design replaces raw pointer-based callbacks with lock-free channels (`crossbeam-channel`). This eliminates lifetime issues, borrow conflicts, and provides real-time safe notification.

## Core Principles

1. **No raw pointers** between trackers and diff trackers
2. **Lock-free notification** via channels
3. **Shared state via Arc** for comparison reads
4. **Deferred processing** via channel drain before resolve
5. **Pre-allocated channels** to avoid allocations during processing

## Architecture

### MidiStateTracker

```rust
use crossbeam_channel::{bounded, Sender, Receiver};
use std::sync::Arc;

// Event sent through channel
#[derive(Clone, Copy)]
pub struct Event {
    pub tracker_id: u64,
    pub event_type: u8,    // 0=note, 1=cc, 2=program, 3=pitch, 4=pressure
    pub channel: u8,
    pub param1: u8,        // note number or CC number
    pub param2: u8,        // velocity or value
}

// Separate data structure for sharing
#[derive(Clone)]
pub struct TrackerStateData {
    pub notes_active: Vec<u8>,         // 16 * 128
    pub controls: Vec<u8>,             // 16 * 128
    pub programs: Vec<u8>,             // 16
    pub pitch_wheel: Vec<u16>,         // 16
    pub channel_pressure: Vec<u8>,     // 16
    pub n_notes_active: i32,
}

pub struct MidiStateTracker {
    id: u64,
    
    // Tracking configuration
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    
    // Owned state - mutated during process_msg
    notes_active: Vec<u8>,
    controls: Vec<u8>,
    programs: Vec<u8>,
    pitch_wheel: Vec<u16>,
    channel_pressure: Vec<u8>,
    n_notes_active: AtomicI32,
    
    // For sharing with diff trackers
    state_data: Arc<TrackerStateData>,
    
    // Lock-free notification channel
    // Pre-allocated capacity - never allocates during send
    sender: Sender<Event>,
}

// Implementation note: Arc<TrackerStateData> is shared immutable view
// owned state is still mutable but kept separate
```

### MidiStateDiffTracker

```rust
use crossbeam_channel::Receiver;

pub struct MidiStateDiffTracker {
    id: u64,
    
    // Per-subscribed-tracker: receiver + shared state
    subscriptions: Vec<Subscription>,
    
    // Diff state
    diffs: BTreeSet<[u8; 2]>,
    
    // Cached trackers for comparison
    tracker_a: Option<(u64, Arc<TrackerStateData>)>,  // (tracker_id, shared_state)
    tracker_b: Option<(u64, Arc<TrackerStateData>)>,
}

struct Subscription {
    tracker_id: u64,
    receiver: Receiver<Event>,
    state: Arc<TrackerStateData>,
}
```

## Key Design Decisions

### 1. Channel-Based Notification

```rust
impl MidiStateTracker {
    pub fn process_note_on(&mut self, channel: u8, note: u8, velocity: u8) {
        if self.notes_active.is_empty() { return; }
        
        let idx = note_index(channel, note);
        let was_inactive = self.notes_active[idx] == NOTE_INACTIVE;
        let old_vel = self.notes_active[idx];
        
        // Update state first
        self.notes_active[idx] = velocity;
        if was_inactive {
            self.n_notes_active.fetch_add(1, Ordering::Relaxed);
        }
        
        // Notify via channel (non-blocking, lock-free)
        // Only if state actually changed
        if was_inactive || old_vel != velocity {
            let event = Event {
                tracker_id: self.id,
                event_type: EVENT_NOTE,
                channel,
                param1: note,
                param2: velocity,
            };
            
            // try_send: atomic CAS, returns immediately
            // If channel full, event dropped (acceptable - sync will rescan)
            let _ = self.sender.try_send(event);
        }
    }
}
```

**Why this works**:
- `try_send()` is atomic CAS operation - no mutex
- Returns immediately - no blocking
- Pre-allocated channel capacity - no allocation
- Non-blocking - if channel full, event dropped

### 2. Subscription via Channel Setup

```rust
impl MidiStateTracker {
    // Called by diff tracker to subscribe
    fn subscribe_to(&self, diff_tracker: &mut MidiStateDiffTracker) {
        // Give diff tracker a clone of the sender and shared state
        let (tx, rx) = bounded::<Event>(CHANNEL_CAPACITY);
        
        diff_tracker.add_subscription(
            self.id,
            rx,
            Arc::clone(&self.state_data)
        );
        
        // Store tx in tracker for later use (or just use internal sender)
        // For simplicity, tracker has one sender, subscribers clone it
    }
}

impl MidiStateDiffTracker {
    fn add_subscription(&mut self, tracker_id: u64, rx: Receiver<Event>, state: Arc<TrackerStateData>) {
        self.subscriptions.push(Subscription {
            tracker_id,
            receiver: rx,
            state,
        });
        
        // Also record which tracker this is (a or b)
        if self.tracker_a.is_none() {
            self.tracker_a = Some((tracker_id, state));
        } else if self.tracker_b.is_none() {
            self.tracker_b = Some((tracker_id, state));
        }
    }
}
```

**Key insight**: Tracker doesn't hold pointers to diff trackers. Diff tracker holds receivers and Arc to tracker state.

### 3. Sync Before Resolve

```rust
impl MidiStateDiffTracker {
    // MUST be called before resolve_to()
    fn sync(&mut self) {
        // Drain ALL event channels completely
        for sub in &mut self.subscriptions {
            while let Ok(event) = sub.receiver.try_recv() {
                self.process_event(&sub.state, event);
            }
        }
    }
    
    fn process_event(&mut self, sender_state: &TrackerStateData, event: &Event) {
        // Get the OTHER tracker's state for comparison
        let other_state = self.get_other_state(event.tracker_id);
        
        match event.event_type {
            EVENT_NOTE => {
                let ch = event.channel;
                let note = event.param1;
                let new_vel = event.param2;
                
                // Compare with OTHER tracker's state
                let other_vel = other_state.notes_active[note_index(ch, note)];
                
                if other_vel != new_vel && new_vel != NOTE_INACTIVE {
                    self.diffs.insert([0x90 | ch, note]);
                } else {
                    self.diffs.remove(&[0x90 | ch, note]);
                }
            }
            // ... similar for CC, program, pitch, pressure
        }
    }
    
    // resolve_to calls sync() first - guaranteed fresh state
    pub fn resolve_to(&mut self, target_id: u64) -> Vec<u8> {
        // CRITICAL: Always sync first to ensure fresh diffs
        self.sync();
        
        // Now generate messages from the "from" tracker to restore "to" tracker
        let from_state = self.get_other_state(target_id);
        let mut out = Vec::new();
        
        for diff in &self.diffs {
            let kind = diff[0] & 0xF0;
            let ch = diff[0] & 0x0F;
            let p1 = diff[1];
            
            match kind {
                0x90 => {
                    let vel = from_state.notes_active[note_index(ch, p1)];
                    if vel != NOTE_INACTIVE {
                        out.push(0x90 | ch);
                        out.push(p1);
                        out.push(vel);
                    }
                }
                // ... similar for other message types
            }
        }
        out
    }
}
```

### 4. Reset with Proper Subscription Management

```rust
impl MidiStateDiffTracker {
    pub fn reset(&mut self, a: &mut MidiStateTracker, b: &mut MidiStateTracker) {
        // Clear old subscriptions
        self.subscriptions.clear();
        
        // Set up new subscriptions (creates channels)
        a.subscribe_to(self);
        b.subscribe_to(self);
        
        // Initial sync to populate diffs
        self.sync();
    }
}
```

## Thread Safety Analysis

### Processing (Audio Thread)

```rust
fn process_note_on(&mut self, channel: u8, note: u8, velocity: u8) {
    // 1. In-place mutation: O(1), no allocation, no mutex
    self.notes_active[idx] = velocity;
    
    // 2. Atomic fetch_add: O(1), lock-free
    if was_inactive {
        self.n_notes_active.fetch_add(1, Ordering::Relaxed);
    }
    
    // 3. try_send: O(1), atomic CAS, no blocking
    let _ = self.sender.try_send(event);
}
```

**Real-time safe**: No allocations, no mutexes, no blocking operations.

### Sync + Resolve (UI Thread or Audio End)

```rust
fn sync(&mut self) {
    for sub in &mut self.subscriptions {
        // try_recv: O(1) per event, non-blocking
        while let Ok(event) = sub.receiver.try_recv() {
            self.process_event(&sub.state, event);
        }
    }
}
```

**Not real-time**: Called between audio buffers, no constraints.

## Memory Layout

```
┌─────────────────────────────────────────────────────────────────┐
│                    MidiStateTracker                              │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ notes_active: Vec<u8>         (owned, mutable)           │   │
│  │ controls: Vec<u8>            (owned, mutable)           │   │
│  │ programs: Vec<u8>             (owned, mutable)           │   │
│  │ pitch_wheel: Vec<u16>        (owned, mutable)           │   │
│  │ channel_pressure: Vec<u8>    (owned, mutable)           │   │
│  └─────────────────────────────────────────────────────────┘   │
│  │ state_data: Arc<TrackerStateData> (shared immutable view) │   │
│  │ sender: Sender<Event>          (lock-free channel send)   │   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Arc::clone() + bounded channel
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   MidiStateDiffTracker                           │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ subscriptions: Vec<Subscription>                        │    │
│  │   - receiver: Receiver<Event>                          │    │
│  │   - state: Arc<TrackerStateData>                      │    │
│  │   - tracker_id: u64                                   │    │
│  └─────────────────────────────────────────────────────────┘    │
│  │ diffs: BTreeSet<[u8; 2]>    (computed on sync)         │    │
└─────────────────────────────────────────────────────────────────┘
```

## Comparison with Original C++

| Aspect | C++ | New Rust |
|--------|-----|----------|
| Tracker→DiffTracker | `weak_ptr.lock()` + direct call | Channel send |
| DiffTracker→Tracker | `shared_ptr` direct access | `Arc<TrackerStateData>` |
| Notification timing | Immediate (synchronous) | Deferred (via channel) |
| sync() requirement | N/A (immediate) | Required before resolve |
| Ownership | `shared_ptr` / `weak_ptr` | `Arc` + channels |
| Thread safety | Assumed single-threaded | Explicit via Arc/channel |
| Real-time safe | Yes (no locks) | Yes (lock-free ops) |

## Deferred Processing Guarantees

### Problem
Events sent via channel aren't processed until `sync()` is called. Between `sync()` and `resolve_to()`, a tracker might process a new message.

### Solution
`sync()` drains ALL channels completely. After `sync()` returns:
1. All pending events are processed
2. No new events exist in any channel
3. New events from subsequent `process_msg()` calls will be in the channel but not yet processed

```rust
pub fn resolve_to(&mut self, target_id: u64) -> Vec<u8> {
    self.sync();  // Drain everything up to this point
    
    // At this moment, all channels are empty.
    // Any process_msg() calls after sync() will queue events.
    // Those events won't be visible until next resolve_to()
    
    // Safe to use diffs now
    self.generate_messages(target_id)
}
```

### Thread Ordering Guarantee

If called from single thread:
```
sync() returns ──► diffs are complete ──► generate_messages()
```

If called from different thread than audio:
```
Audio thread: process_msg() ──► try_send() ──► event in channel
                                                      │
UI thread:    sync() ──► try_recv() ──► drain ──► diffs updated
```

Events sent after `sync()` starts but before it completes may or may not be included (race), but this is acceptable as they represent state changes that occurred during the sync itself.

## Dependencies

```toml
# Cargo.toml
[dependencies]
crossbeam-channel = "0.5"  # Lock-free channels
```

`crossbeam-channel` provides:
- Bounded channels with pre-allocated capacity
- `try_send()` / `try_recv()` non-blocking operations
- Atomic CAS for queue operations
- No heap allocation after initialization

## References

- Original C++ design: See `cpp_design.md`
- Current broken implementation: See `rust_design_current.md`
- Implementation task: See `task.md`
