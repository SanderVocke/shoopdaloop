# MIDI Time Field Simplification

## Overview

Simplify `MidiStorageElem` to use a single `time` field instead of two (`proc_time`, `storage_time`). The distinction between absolute (storage) time and relative (buffer) time exists only during the boundary crossing between storage and real-time buffers — not within the struct itself.

---

## Current State

`MidiStorageElem` currently has two time fields:

```cpp
struct MidiStorageElem {
    uint32_t storage_time;  // Absolute time in loop storage
    uint16_t proc_time;     // Time relative to current process buffer
    uint16_t size;
    uint8_t bytes[4];
};
```

### Why Two Fields?

They were needed because messages could be passed by reference:
- `storage_time` preserved the original recording position
- `proc_time` was set dynamically for real-time buffers

When passing by reference, you couldn't modify the message without corrupting storage.

---

## Proposed State

A single `time` field:

```cpp
struct MidiStorageElem {
    uint32_t time;      // Universal time field
    uint16_t size;
    uint8_t bytes[4];
};
```

### How It Works

The `time` field always represents the **position the message was recorded at**. When a message is **sent to a real-time buffer**, the caller computes the relative time on-the-fly:

```cpp
// When sending stored message to output buffer:
int buffer_rel_time = event.time - current_loop_pos + n_frames_processed;
MidiStorageElem out = event;
out.time = buffer_rel_time;  // overwrite with relative time for this buffer
write_event(out);
```

### The Rule

> **Conversion happens at the storage boundary, not inside MidiStorageElem.**

| Context | Time Value Meaning | Who Sets It |
|---------|-------------------|-------------|
| Recording | Absolute (loop position) | Source buffer |
| Storage | Absolute (loop position) | MidiChannel::record |
| Retrieval (via API) | Absolute (loop position) | MidiStorage (unchanged) |
| Sending to output | Relative (buffer position) | MidiChannel::playback |

---

## Conversion Rules

### Recording

```cpp
// Source buffer has proc_time (relative to its buffer)
// Storage wants absolute position
auto event = input_buffer.get_event(i);
storage.append(
    record_from + event.time,  // convert to absolute position
    event.size,
    event.bytes
);
```

### Playback

```cpp
// Storage has absolute position (time)
// Output buffer wants relative position
int relative_time = event.time - current_position + frames_processed;
output_buffer.write_event(make_event(relative_time, event.size, event.bytes));
```

### API Return

```cpp
// Contents retrieved from storage:
// - time field contains absolute storage position
// - This is what we return to the user
auto contents = channel.retrieve_contents();
for (auto& msg : contents.recorded_msgs) {
    // msg.time is the absolute position — correct for user-facing API
}
```

---

## Benefits

1. **Simpler struct** — one less field to understand and maintain
2. **Clearer semantics** — `time` always means "position in storage"
3. **Explicit conversion** — time transformation is visible at call sites, not hidden in message struct
4. **Immutability-friendly** — the single `time` value can remain const in storage context

---

## Required Changes

### 1. MidiStorageElem (MidiStorage.h)

```cpp
// Before
struct MidiStorageElem {
    uint32_t storage_time;
    uint16_t proc_time;
    uint16_t size;
    uint8_t bytes[4];
};

// After
struct MidiStorageElem {
    uint32_t time;      // Single time field
    uint16_t size;
    uint8_t bytes[4];
};
```

### 2. MidiStorage (storage operations)

- Update `append()` signature: `time` instead of `storage_time`
- Update `for_each_msg()`: yields `time` instead of `storage_time`
- No other changes needed — internal storage was already using the right field

### 3. MidiChannel recording

```cpp
// Before
storage.append(record_from + event.proc_time - recbuf.first.n_frames_processed, ...);

// After (same logic, different field name)
storage.append(record_from + event.time - recbuf.first.n_frames_processed, ...);
```

### 4. MidiChannel playback

```cpp
// Before (in PROC_process_playback)
event->proc_time = proc_time;

// After
MidiStorageElem out = *event;
out.time = proc_time;  // Overwrite with buffer-relative time
write_event(out);

// Or, if caller needs absolute time for API later:
MidiStorageElem out;
out.time = event->time;              // Original absolute time for API
out.size = event->size;
out.bytes = event->bytes;
// ... then set up relative time separately for buffer
```

### 5. MidiSortingBuffer

- Replace `proc_time` comparisons with `time`
- Same sorting logic, just field rename

### 6. JackMidiPort Input

```cpp
// Before
elem.proc_time = jack_event.time;

// After
elem.time = jack_event.time;
```

### 7. JackMidiPort Output

```cpp
// When writing to JACK buffer
API::midi_event_write(jack_buffer, event.time, event.bytes, event.size);
// Note: JACK expects relative time, so conversion must happen before calling write_event()
```

### 8. DecoupledMidiPort

- `DecoupledMidiMessage` uses single `time` field
- No behavioral change — queue was already discarding time info

### 9. API (libshoopdaloop_backend.cpp)

```cpp
// external_midi_data() - return recorded messages
// Before: m.recorded_msgs[idx].storage_time
// After:  m.recorded_msgs[idx].time

// internal_midi_data() - load messages into storage
// Before: m.storage_time = from.time
// After:  m.time = from.time
```

### 10. Tests

Update any references to `proc_time` or `storage_time` to use `time`. When comparing output buffer messages against expected values, the comparison should use the `time` value that was set for that buffer (computed relative to buffer start).

---

## What Stays the Same

- The `MidiStorage` append/for_each interface (only field name changes)
- The `MidiReadableBuffer` / `MidiWriteableBuffer` interfaces (already use `MidiStorageElem` directly)
- The copy-only semantics (no pass-by-reference)
- The fixed 4-byte inline payload

---

## Summary

| Before | After |
|--------|-------|
| Two time fields | One time field |
| Time semantics unclear | `time` = absolute storage position |
| Conversion implicit | Conversion explicit at boundary |
| `MidiStorageElem` changed during playback | `MidiStorageElem` immutable in storage, converted at send site |