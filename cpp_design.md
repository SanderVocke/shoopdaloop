# C++ MidiTracker Design Document

## Overview

The C++ implementation uses a callback-based observer pattern with shared pointers for lifetime management. There are two main classes: `MidiStateTracker` and `MidiStateDiffTracker`.

## MidiStateTracker

### Purpose
Tracks MIDI state (notes active, CC values, pitch wheel, channel pressure, programs) for a set of channels.

### Lifetime & Ownership
- **Managed by**: `std::shared_ptr<MidiStateTracker>` (via `shoop_shared_ptr`)
- **Self**: Inherits from `std::enable_shared_from_this<MidiStateTracker>` to pass `shared_ptr` to subscribers
- **Subscribers**: Held as `std::set<std::weak_ptr<Subscriber>>` - weak references to avoid cycles

### Behavior

#### State Storage
```cpp
std::atomic<int> m_n_notes_active;
std::vector<uint8_t> m_notes_active_velocities;  // 16 * 128 entries
std::vector<uint8_t> m_controls;                 // 16 * 128 entries
std::vector<uint8_t> m_programs;                 // 16 entries
std::vector<uint16_t> m_pitch_wheel;             // 16 entries
std::vector<uint8_t> m_channel_pressure;         // 16 entries
```

#### Processing
```cpp
void MidiStateTracker::process_noteOn(uint8_t channel, uint8_t note, uint8_t velocity) {
    // 1. Check if tracking is enabled
    if (m_notes_active_velocities.size() == 0) return;
    
    // 2. Update internal state
    if (m_notes_active_velocities.at(note_index(channel, note)) == NoteInactive) {
        m_n_notes_active++;
    }
    
    // 3. Notify subscribers if value changed
    if (m_notes_active_velocities.at(note_index(channel, note)) != velocity) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->note_changed(this, channel, note, velocity);  // Direct callback
            }
        }
    }
    
    // 4. Update state
    m_notes_active_velocities.at(note_index(channel, note)) = velocity;
}
```

### Key Design Decisions

1. **Direct callbacks**: When state changes, `note_changed()` is called immediately on all subscribers
2. **State update after notification**: Subscribers see the OLD state, then state is updated
3. **Weak pointers**: Prevents memory leaks from subscriber objects that outlive their owners
4. **Tracking flags**: Only process if tracking is enabled (vector has non-zero size)

## MidiStateDiffTracker

### Purpose
Tracks the difference between two `MidiStateTracker` instances. Used to determine what MIDI messages are needed to make one tracker match another.

### Lifetime & Ownership
- **Managed by**: `std::shared_ptr<MidiStateDiffTracker>` (via `shoop_enable_shared_from_this`)
- **Trackers**: Holds `std::shared_ptr<MidiStateTracker>` (m_a, m_b)
- **Self**: Can pass `shared_from_this()` to trackers for subscription

### Behavior

#### State Storage
```cpp
std::shared_ptr<MidiStateTracker> m_a;
std::shared_ptr<MidiStateTracker> m_b;
boost::container::flat_set<DifferingState> m_diffs;  // {(msg_byte0, byte1), ...}
```

#### Subscription Management
```cpp
void MidiStateDiffTracker::reset(
    std::shared_ptr<MidiStateTracker> a,
    std::shared_ptr<MidiStateTracker> b,
    StateDiffTrackerAction action
) {
    // 1. Unsubscribe from old trackers
    if (m_a) m_a->unsubscribe(shared_from_this());
    if (m_b) m_b->unsubscribe(shared_from_this());
    
    // 2. Set new trackers
    m_a = a;
    m_b = b;
    
    // 3. Subscribe to new trackers
    if (m_a) m_a->subscribe(shared_from_this());
    if (m_b) m_b->subscribe(shared_from_this());
    
    // 4. Perform requested action
    switch (action) {
        case StateDiffTrackerAction::ScanDiff: rescan_diff(); break;
        case StateDiffTrackerAction::ClearDiff: clear_diff(); break;
    }
}
```

#### Notification Handling (Callback)
```cpp
void MidiStateDiffTracker::note_changed(
    MidiStateTracker* tracker,
    uint8_t channel,
    uint8_t note,
    std::optional<uint8_t> maybe_velocity
) {
    // 1. Verify this tracker is one we're tracking
    if (tracker != m_a.get() && tracker != m_b.get()) return;
    
    // 2. Get the OTHER tracker for comparison
    auto& other = tracker == m_a.get() ? m_b : m_a;
    
    // 3. If other tracker has different state, record a diff
    if (other && other->tracking_notes() &&
        other->maybe_current_note_velocity(channel, note) != maybe_velocity) {
        m_diffs.insert({(uint8_t)(0x90 | channel), note});
    } else {
        m_diffs.erase({(uint8_t)(0x90 | channel), note});
    }
}
```

**Critical behavior**: When a tracker reports a note change, the diff tracker compares with the OTHER tracker's current state. This is why it needs access to both trackers.

#### Resolving
```cpp
void MidiStateDiffTracker::resolve_to(
    MidiStateTracker* to,
    std::function<void(uint32_t size, uint8_t* data)> put_message_cb,
    bool notes, bool controls, bool programs
) {
    // 1. Verify valid target
    if (to != m_a.get() && to != m_b.get()) return;
    if (!m_a || !m_b) return;
    
    // 2. Determine source (FROM which tracker)
    auto& from = to == m_a.get() ? *m_b : *m_a;
    
    // 3. Generate messages from FROM state to restore TO state
    for (auto& d : m_diffs) {
        // For each differing element, look up the value in FROM tracker
        // and generate a message that will make TO match
    }
}
```

## Usage Pattern

### Recording Phase
```cpp
// Create trackers
auto prev_state = std::make_shared<MidiStateTracker>(true, true, false);
auto recording = std::make_shared<MidiStateTracker>(true, true, false);

// Create diff tracker between them
auto diff_tracker = std::make_shared<MidiStateDiffTracker>();
diff_tracker->reset(prev_state, recording);

// During recording:
recording->process_msg(midi_data);
// prev_state may also process (e.g., loop start state)

// Diff tracker automatically records differences via callbacks
```

### Playback Phase
```cpp
// At loop boundary:
diff_tracker->resolve_to(prev_state, callback);
// Generates messages to restore prev_state based on current recording
```

## Thread Safety Considerations

- **Assumed single-threaded processing**: No locks in the tracker classes
- **Weak pointers**: Thread-safe `lock()` operation
- **Atomic counter**: `m_n_notes_active` uses atomic operations
- **No mutex in diff tracker**: All operations assumed to happen on audio thread

## Relationships Diagram

```
MidiStateTracker                     MidiStateDiffTracker
┌─────────────────────┐             ┌─────────────────────────────┐
│ m_notes_active[...]  │             │ m_a ────────────────────────┼──► shared_ptr<Tracker>
│ m_controls[...]      │             │ m_b ────────────────────────┼──► shared_ptr<Tracker>
│ m_subscribers ───────┼──► weak_ptr │ m_diffs ────────────────────┼──► flat_set
└─────────────────────┘             └─────────────────────────────┘
         │                                      ▲
         │ weak_ptr.lock()                      │
         └──────────────────────────────────────┘
                     subscribe(shared_from_this())
```

## Summary of Ownership Model

| Entity | Owns | References |
|--------|------|------------|
| `MidiStateTracker` | Its own state | `weak_ptr` to subscribers |
| `MidiStateDiffTracker` | Diff set, shared_ptr to trackers | `shared_ptr` to trackers |
| Caller | Manages lifetimes | `shared_ptr` to both |

The design relies on:
1. **C++ destructors** for cleanup when shared_ptr counts hit zero
2. **Weak pointers** to prevent circular reference leaks
3. **Synchronous callbacks** for immediate state synchronization
4. **Single-threaded processing** (no locks needed)
