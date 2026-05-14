# MIDI Buffer Refactoring Design Document

## 1. Overview

This document outlines a plan to simplify the MIDI processing architecture in ShoopDaLoop's C++ backend by:
1. **Limiting all MIDI messages to a maximum of 4 bytes** — enabling fixed-size storage with inline data
2. **Eliminating all "pass by reference" mechanisms** — all messages are copied between buffers

This change improves code decoupling, maintainability, and understandability while maintaining real-time performance.

## 2. Background: The Current Architecture

### 2.1 The Problem Space

MIDI messages flow through the system from external sources through processing to storage and output. Different sources have different memory ownership characteristics:

| Source | Owns Data? | Can Pass by Reference? | Can Pass by Value? |
|--------|-----------|----------------------|-------------------|
| JACK MIDI (transient) | No | Only during process cycle | Yes (copy) |
| MidiChannel storage | Yes | No (data changes) | Yes (copy needed) |
| DummyMidiPort | Yes | Yes | Yes |

### 2.2 Current Interface Hierarchy

The current system uses several abstract interfaces:

```
MidiSortableMessageInterface
    └── get_time(), get_data(), get_size(), get()
         (abstract message representation)

MidiReadableBufferInterface
    └── PROC_get_event_reference()  — get pointer to message
    └── PROC_get_event_value()      — copy message data
    └── read_by_reference_supported()

MidiWriteableBufferInterface
    └── PROC_write_event_reference() — write pointer to buffer
    └── PROC_write_event_value()      — copy data to buffer
    └── write_by_reference_supported()
    └── write_by_value_supported()

MidiReadWriteBufferInterface — combines both
```

### 2.3 Current Message Types

| Type | Max Size | Storage | Purpose |
|------|----------|---------|---------|
| `MidiMessage<TimeType, SizeType>` | Variable | `std::vector<uint8_t>` heap-allocated | General purpose (being phased out) |
| `MaxSizeMidiMessage<MaxDataSize>` | 3 bytes | Inline `uint8_t[MaxDataSize]` | Sorting buffer internal |
| `MidiStorageElem` | 4 bytes | Inline `uint8_t[4]` | Long-term storage (channel, ringbuffer) |
| `MidiBufferingInputPort::ReadMessage` | Variable | Pointer to external data | Wrapping JACK input |

### 2.4 Current Data Flow

```
External MIDI (JACK/Dummy)
        ↓
MidiPort::PROC_get_read_output_data_buffer()
        ↓ (by reference if source owns, by value if not)
MidiSortingBuffer — stores pointers OR copies
        ↓ (sorted during PROC_process)
GraphMidiPort → GraphLoop → MidiChannel
        ↓ (playback, tries reference, falls back to value)
OutputPort buffer (JACK or Dummy)
```

### 2.5 The Two-Time-Field Problem

The current `MidiStorageElem` has two time fields:
- `storage_time`: Absolute time in storage context (loop position)
- `proc_time`: Time relative to current process iteration

These were needed because of the pass-by-reference mechanism:
- Storage needed to preserve absolute time for long-term storage
- Real-time processing needed time relative to the current buffer

When passing by reference, messages from storage couldn't be modified (would corrupt storage). When passing by value, we can freely transform the message when copying.

## 3. The Proposed Architecture

### 3.1 Single Canonical Message Type

After refactoring, `MidiStorageElem` becomes the **single canonical MIDI message struct** for the entire system:

```cpp
struct MidiStorageElem {
    uint32_t storage_time;  // Absolute time in storage context
    uint16_t proc_time;    // Time relative to process iteration
    uint16_t size;          // 1-4 bytes
    uint8_t bytes[4];       // Inline fixed-size payload
};
```

**Key properties:**
- Maximum 4 bytes of data — covers all standard MIDI messages (status + 1-3 data bytes)
- Fixed-size: enables flat arrays, no pointer chasing, excellent cache locality
- Inline storage: no heap allocations
- Two time fields serve different purposes (see below)

**Time Field Semantics:**
Since all messages are now **copied** rather than passed by reference, the two time fields serve distinct purposes:
- `storage_time`: Used when storing/reporting message positions in the loop storage context. Set once when message is recorded and preserved in storage.
- `proc_time`: Used when placing messages in real-time processing buffers. Set dynamically when copying from storage to a real-time output buffer.

**Conversion from Storage to Real-time:**
When `MidiChannel` sends stored messages to a `MidiWriteableBuffer`, it must convert `storage_time` to `proc_time`:
```cpp
// When sending stored message to real-time buffer:
event.proc_time = event.storage_time - current_buffer_position + n_frames_processed;
```

### 3.2 Simplified Buffer Interfaces

Replace 4 interfaces with 2:

```cpp
class MidiReadableBuffer {
public:
    virtual uint32_t n_events() const = 0;
    virtual MidiStorageElem get_event(uint32_t idx) const = 0;  // returns copy
    virtual ~MidiReadableBuffer() {}
};

class MidiWriteableBuffer {
public:
    virtual void write_event(MidiStorageElem event) = 0;  // receives copy
    virtual ~MidiWriteableBuffer() {}
};
```

**Key changes:**
- No `read_by_reference_supported()` — always copy
- No `write_by_reference_supported()` — always copy
- No `MidiSortableMessageInterface` — `MidiStorageElem` is concrete, not abstract

### 3.3 New Data Flow

```
JACK MIDI (raw)
        ↓ (immediate copy into MidiStorageElem)
Input Port (stores vector<MidiStorageElem>)
        ↓ (std::stable_sort by time)
MidiChannel (plays back from storage, always copies)
        ↓
Output Port (always copies)
        ↓ (immediate copy)
JACK MIDI output
```

All copies, all fixed-size, all simple.

## 4. Classes to Modify

### 4.1 `MidiStorageElem` (`MidiStorage.h`)

**Current state:** Already using inline 4-byte storage. Inherits from `MidiSortableMessageInterface`.

**Changes:**
- Remove `MidiSortableMessageInterface` inheritance (no longer needed)
- Keep existing inline storage (`bytes[4]`) — already correct
- Add convenience constructors/conversions if helpful

**End state:** Standalone struct, the canonical message type.

---

### 4.2 New `MidiBuffer.h` (replaces `MidiBufferInterfaces.h`)

**Current state:** Four interfaces (`MidiSortableMessageInterface`, `MidiReadableBufferInterface`, `MidiWriteableBufferInterface`, `MidiReadWriteBufferInterface`).

**Changes:**
- Define `MidiReadableBuffer` (copy-only read)
- Define `MidiWriteableBuffer` (copy-only write)
- Delete all old interfaces

**End state:** Simple, small, two-interface file.

---

### 4.3 `MidiSortingBuffer` (`MidiSortingBuffer.h/cpp`)

**Current state:**
```cpp
std::vector<const MidiSortableMessageInterface*> references;  // pointer storage
std::vector<Message> stored_messages;                         // owned copies
bool dirty = false;
```

**Changes:**
- Remove `references` vector (no pointer-based sorting)
- Keep `stored_messages` as `std::vector<MidiStorageElem>`
- Remove `PROC_write_event_reference()` — only `PROC_write_event()`
- Remove `dirty` flag — sort before access, simpler
- Simplify comparator to `([](const MidiEvent& a, const MidiEvent& b) { return a.time < b.time; })`

**End state:** Simple vector with sort. Implements `MidiReadableBuffer` and `MidiWriteableBuffer`.

---

### 4.4 `MidiBufferingInputPort` (`MidiBufferingInputPort.h/cpp`)

**Current state:** Wraps external JACK data in `ReadMessage` structs that store raw pointers to transient data.

**Changes:**
- Delete entire class
- Move copy logic directly into `GenericJackMidiInputPort`

**End state:** Removed. Logic inlined where needed.

---

### 4.5 JACK Ports (`jack/JackMidiPort.h/cpp`)

**Current state:** `GenericJackMidiInputPort` inherits from `MidiBufferingInputPort`. `GenericJackMidiOutputPort` inherits from `MidiSortingReadWritePort`.

**Changes for input port:**
- Remove `MidiBufferingInputPort` inheritance
- Add `std::vector<MidiStorageElem> m_messages` member
- In `PROC_process()`: copy JACK events directly into `MidiStorageElem`
- Implement `MidiReadableBuffer` directly

**Changes for output port:**
- Remove `MidiSortingReadWritePort` inheritance
- Implement `MidiWriteableBuffer` directly
- In `write_event()`: copy `MidiStorageElem` to JACK buffer

**End state:** Self-contained ports, no intermediate buffer classes.

---

### 4.6 `MidiChannel` (`MidiChannel.h/cpp`)

**Current state:**
```cpp
void PROC_send_message_ref(MidiWriteableBufferInterface &buf, 
                            MidiSortableMessageInterface const &event);
void PROC_send_message_value(MidiWriteableBufferInterface &buf, 
                              uint32_t time, uint32_t size, uint8_t *data);
```

**Changes:**
- Remove `PROC_send_message_ref()` — no longer needed
- Keep `PROC_send_message_value()` as single `send()` method
- Or rename to `PROC_send_message(MidiWriteableBuffer& buf, MidiStorageElem event)`
- Remove `MidiSortableMessageInterface` include
- **Important**: When sending stored messages to output buffer, convert `storage_time` to `proc_time`:
  ```cpp
  void MidiChannel::PROC_send_message(MidiWriteableBuffer &buf, MidiStorageElem event) {
      // Convert storage time to process time for realtime buffer
      event.proc_time = event.storage_time - our_pos + buf.n_frames_processed;
      buf.write_event(event);
      mp_output_midi_state->process_msg(event.bytes);
  }
  ```

**End state:** One send method, always copies, with correct time conversion.

---

### 4.7 `DummyMidiPort` (`DummyMidiPort.h/cpp`)

**Current state:** Implements both reference and value interfaces, complex logic around queuing.

**Changes:**
- Remove `write_by_reference_supported()`
- Remove `write_by_value_supported()`
- Remove `PROC_write_event_reference()`
- Replace with single `write_event(MidiStorageElem)`
- Simplify internal storage to `std::vector<MidiStorageElem>`

**End state:** Simplified, copy-only.

---

### 4.8 `DummyMidiBufs` (`DummyMidiBufs.h/cpp`)

**Current state:** Exists solely to provide reference interfaces for testing.

**Changes:**
- Delete entirely

**End state:** Removed.

---

### 4.9 `MidiSortingReadWritePort` (`MidiSortingReadWritePort.h`)

**Current state:** Wraps `MidiSortingBuffer` with interface passthrough.

**Changes:**
- Delete entirely
- Ports implement interfaces directly

**End state:** Removed.

---

### 4.10 `DecoupledMidiPort` (`DecoupledMidiPort.h/cpp`)

**Current state:**
```cpp
buf->PROC_get_event_reference(idx).get(size, time, data);  // read by reference
m.data = std::vector<uint8_t>(size);                       // then copy
```

**Changes:**
- Change to `auto event = buf.get_event(idx);` (already a copy)
- Use `MidiStorageElem` directly instead of `DecoupledMidiMessage` wrapper

**End state:** Simpler, uses canonical message type.

---

### 4.11 `MidiPort` (`MidiPort.h/cpp`)

**Current state:** Five atomic buffer pointers, many virtual methods for buffer retrieval.

**Changes:**
- Simplify to basic `MidiReadableBuffer*` and `MidiWriteableBuffer*` output/input
- Remove references to removed interfaces
- Keep essential port functionality (muting, state tracking, ringbuffer snapshot)

**End state:** Simplified, delegates to concrete buffer implementations.

---

### 4.12 `MidiStorage`, `MidiRingbuffer` (`MidiStorage.h/cpp`, `MidiRingbuffer.h/cpp`)

**Current state:** Already using `MidiStorageElem` with inline 4-byte storage.

**Changes:**
- Minimal changes — already compatible with new design
- May need to update interface compliance (implement `MidiReadableBuffer` directly)
- Remove any remaining references to `MidiSortableMessageInterface`

**End state:** Already mostly correct, minor interface updates.

---

### 4.13 `InternalLV2MidiOutputPort` (`lv2/InternalLV2MidiOutputPort.h/cpp`)

**Current state:** Implements `MidiWriteableBufferInterface` with both reference and value methods.

**Changes:**
- Remove `PROC_write_event_reference()`
- Remove `write_by_reference_supported()` and `write_by_value_supported()`
- Implement simple `write_event(MidiStorageElem)`

**End state:** Simplified.

---

### 4.14 `MidiMessage.h`

**Current state:** Defines `MidiMessage` (heap-allocated, variable size) and `MaxSizeMidiMessage`.

**Changes:**
- Delete entire file
- `MidiStorageElem` replaces all use cases

**End state:** Removed.

---

## 5. Classes to Delete Entirely

| File | Reason |
|------|--------|
| `MidiBufferInterfaces.h` | Replaced by new `MidiBuffer.h` (2 interfaces vs 4) |
| `MidiMessage.h` | Replaced by `MidiStorageElem` |
| `MidiSortingReadWritePort.h` | Logic moved into ports directly |
| `MidiBufferingInputPort.h/cpp` | Logic moved into JACK input port |
| `DummyMidiBufs.h/cpp` | Only existed for reference interfaces |

---

## 6. Performance Analysis

### 6.1 Gains

1. **No pointer chasing in sorting**
   - Currently: sort `const MidiSortableMessageInterface*` pointers, then dereference
   - After: sort `MidiStorageElem` directly in flat vector
   - Better cache locality, simpler code

2. **Eliminates "dirty" re-sorting**
   - Current: dirty flag checked on every access
   - After: sort once before access, simpler control flow

3. **Eliminates interface dispatch overhead**
   - Every `PROC_get_event_reference()` is a virtual call
   - After: `get_event()` is still virtual but struct is small and inlineable

4. **No conditional branching on "supported" methods**
   - `MidiChannel::PROC_send_message_ref()` checks two bools then dispatches
   - After: just `write_event()`

5. **Single memory allocation path**
   - `MidiMessage` with `std::vector` allocation eliminated
   - `MidiStorageElem` always stack-allocated then moved into vector

### 6.2 Costs

1. **Always copying**
   - For typical loop with 1000 messages × 3 bytes = 3KB memcpy
   - Modern CPUs: nanoseconds
   - **Acceptable for real-time constraints**

2. **std::stable_sort() vs pointer swap**
   - Larger sort items (12 bytes vs 8 bytes per pointer)
   - Cache behavior remains good for small structs

3. **No pre-sorted buffer sharing**
   - Currently: JACK input → reference directly to sorting buffer (no copy)
   - After: always copy into `MidiStorageElem`
   - **Main cost, but acceptable**

### 6.3 Real-Time Safety

`PROC_process()` is already in the real-time thread. With typical max ~1000 messages per cycle:
- 1000 × 12-byte struct copy ≈ 12KB
- memcpy of 12KB: ~50 nanoseconds
- **Well within real-time budget**

---

## 7. Build and Test

### 7.1 Building

From the repository root:
```bash
cargo build
```

This builds the entire project (Rust + C++ via cargo-cxx).

### 7.2 Testing

```bash
# Unit tests
cargo test

# Integration tests (C++ test runner)
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner
```

Or find the installed version:
```bash
./target/debug/build/backend-*/out/cmake_install/tools/shoopdaloop/test_runner
```

---

## 8. Summary of Simplification

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Message abstraction layers | 4 interfaces | `MidiStorageElem` (concrete) | -75% |
| Buffer interface methods | 8+ virtual methods | 2 virtual methods | -75% |
| Pass-by-reference code paths | 8+ call sites | 0 | -100% |
| Classes/files for MIDI buffers | ~15 | ~8 | -47% |
| Max MIDI message size | Variable (enforced 3 in sorting) | 4 (fixed) | Defined |
| Memory allocation per message | 0-1 (heap for MidiMessage) | 0 (stack then vector) | Eliminated |

---

## 9. Implementation Order

See `todo.md` for detailed task list. High-level order:

1. Create new `MidiBuffer.h` with simplified interfaces
2. Update `MidiStorageElem` to remove old interface dependency
3. Update `MidiSortingBuffer` (simplify storage, remove reference support)
4. Update `MidiChannel` (single send method)
5. Update `DummyMidiPort` (remove reference support)
6. Update JACK ports (inline buffering, remove intermediate classes)
7. Update `DecoupledMidiPort` (use canonical message type)
8. Update LV2 port
9. Delete removed files
10. Update tests
11. Full build and test verification