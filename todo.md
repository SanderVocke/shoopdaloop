# MIDI Buffer Refactoring: Task List

## Overview

This document contains the sequential todo list for refactoring the MIDI processing architecture:
- Limit all MIDI messages to maximum 4 bytes
- Eliminate all "pass by reference" mechanisms (copy-only)

For details, see `plan.md`.

---

## Phase 1: Foundation (New Interfaces and Core Types)

### [x] 1.1: Create new `src/backend/internal/MidiBuffer.h`

**Goal:** Define simplified buffer interfaces replacing `MidiBufferInterfaces.h`

**Changes:**
- Define `MidiReadableBuffer` class with:
  - `virtual uint32_t n_events() const = 0`
  - `virtual MidiStorageElem get_event(uint32_t idx) const = 0` (returns copy)
- Define `MidiWriteableBuffer` class with:
  - `virtual void write_event(MidiStorageElem event) = 0`
- Do NOT define `MidiSortableMessageInterface` — not needed

**End state:** New file exists, old interfaces still in place for compatibility.

---

### [x] 1.2: Update `MidiStorageElem` in `MidiStorage.h`

**Goal:** Make `MidiStorageElem` the canonical MIDI message type, remove old interface dependency

**Changes:**
- Remove `#include "MidiBufferInterfaces.h"`
- Remove `MidiSortableMessageInterface` inheritance (wait - actually kept for compatibility)
- Keep existing inline 4-byte storage (`bytes[4]`) — already correct
- Add `operator==()` for comparison (needed for tests)

**End state:** `MidiStorageElem` has operator== for comparison.

**Verification:**
- `cargo build` succeeds
- `cargo test` passes

---

### [x] 1.3: Update `MidiStorage.h` includes

**Goal:** Remove references to deleted `MidiBufferInterfaces.h`

**Changes:**
- Update `#include` directives throughout the file
- Verify `MidiStorage`, `MidiStorageCursor`, `MidiRingbuffer` still compile

**End state:** No references to `MidiBufferInterfaces.h` in `MidiStorage.h`.

**Verification:**
- `cargo build` succeeds

---

## Phase 2: Buffer Implementations

### [x] 2.1: Refactor `MidiSortingBuffer`

**Goal:** Simplify to single vector storage, remove reference support

**Changes in `MidiSortingBuffer.h`:**
- Remove `std::vector<const MidiSortableMessageInterface*> references`
- Keep `std::vector<Message> stored_messages` (where `Message = MaxSizeMidiMessage<uint32_t, uint32_t, 3>`)
- Change to `std::vector<MidiStorageElem> messages`
- Remove `bool dirty` member
- Remove `bool write_by_reference_supported()` declaration
- Remove `bool read_by_reference_supported()` declaration
- Remove `PROC_write_event_reference()` declaration
- Add `void write_event(MidiStorageElem event)` declaration

**Changes in `MidiSortingBuffer.cpp`:**
- Remove `compare` struct (pointer-based comparator)
- Implement new `write_event()` method (copy into vector)
- Simplify `PROC_sort()` to sort `std::vector<MidiStorageElem>` directly
- Remove dirty flag checks
- Update `PROC_get_event_reference()` → `get_event()` returning copy
- Update `PROC_get_event_value()` → use new `get_event()`

**End state:** Single vector, copy-only interface.

**Verification:**
- `cargo build` succeeds
- `cargo test` passes

---

### [x] 2.2: Update `MidiPort.h`

**Goal:** Simplify buffer pointer storage, remove old interface references

**Changes:**
- Remove atomic `MidiWriteableBufferInterface*` and `MidiReadableBufferInterface*` pointers
- Add simple buffer access methods or direct `MidiStorageElem` vectors
- Update `PROC_get_*` method signatures to new interfaces
- Remove references to `MidiBufferInterfaces.h`

**End state:** `MidiPort` uses simplified buffer access.

**Verification:**
- `cargo build` succeeds

---

### [x] 2.3: Update `MidiPort.cpp`

**Goal:** Implement simplified buffer retrieval

**Changes:**
- Update `PROC_get_*` methods to match new signatures
- Ensure ringbuffer snapshot functionality still works

**End state:** Compiles with new interfaces.

**Verification:**
- `cargo build` succeeds

---

### [x] 2.4: Update `DummyMidiPort`

**Goal:** Remove reference support, use simplified copy-only interface

**Changes in `DummyMidiPort.h`:**
- Remove `MidiReadableBufferInterface` and `MidiWriteableBufferInterface` inheritance
- Add `#include "MidiBuffer.h"` (new)
- Keep `std::vector<StoredMessage>` where `StoredMessage = MidiStorageElem`
- Remove `bool write_by_reference_supported() const override`
- Remove `bool write_by_value_supported() const override`
- Remove `bool read_by_reference_supported() const override`
- Remove `PROC_write_event_reference()` override
- Add `void write_event(MidiStorageElem event)` method

**Changes in `DummyMidiPort.cpp`:**
- Remove `PROC_write_event_reference()` implementation
- Remove `PROC_write_event_value()` implementation (or rename)
- Implement new `write_event()` method
- Update `PROC_get_event_reference()` → `get_event()` returning copy
- Update `PROC_get_n_events()` to use new storage
- Remove stable_sort with pointer comparator — use direct comparison
- Remove references to `MidiSortableMessageInterface`

**End state:** Copy-only, simplified logic.

**Verification:**
- `cargo build` succeeds
- `cargo test` passes

---

## Phase 3: Message Sending

### [x] 3.1: Update `MidiChannel.h`

**Goal:** Single send method, remove reference-based overload

**Changes:**
- Remove `PROC_send_message_ref()` declaration
- Keep `PROC_send_message()` taking `MidiStorageElem` and `MidiWriteableBuffer&`
- Add `#include "MidiBuffer.h"`
- Remove `#include "MidiMessage.h"`
- Update `Contents::recorded_msgs` to use `std::vector<MidiStorageElem>` instead of `std::vector<Message>`

**End state:** One send method, `Contents` uses `MidiStorageElem`.

**Verification:**
- `cargo build` succeeds

---

### [x] 3.2: Update `MidiChannel.cpp`

**Goal:** Implement single send method, update all call sites

**Changes:**
- Update `retrieve_contents()` to build `MidiStorageElem` instead of `MidiMessage`
- Update `set_contents()` to accept `MidiStorageElem` and use `storage_time` field
- Update `PROC_send_message()` to pass `MidiStorageElem` directly to `write_event()`
- Update `PROC_send_all_sound_off()` to use `MidiStorageElem`
- Remove `MidiMessage` usage

**End state:** All message sends go through single method.

**Verification:**
- `cargo build` succeeds

---

### [x] 3.3: Update API binding (`libshoopdaloop_backend.cpp`)

**Goal:** Update FFI bindings to work with `MidiStorageElem`

**Changes:**
- Update `external_midi_data()`: use `storage_time` instead of `time`, `bytes` instead of `data`
- Update `internal_midi_data()`: build `MidiStorageElem` instead of `_MidiMessage`

**End state:** API bindings work with new message type.

**Verification:**
- `cargo build` succeeds

---

### [x] 3.4: Fix storage-to-realtime time conversion

**Goal:** Correctly compute `proc_time` when sending stored messages to real-time buffers

**Background:**
When sending stored messages to a real-time buffer (e.g., during playback), the message must have:
- `proc_time`: Time relative to the current process buffer (set when copying to output)
- `storage_time`: Original time in loop storage (preserved in storage, not sent)

The `MidiChannel::PROC_process_playback()` currently sends messages with their `storage_time` as-is, which is incorrect for real-time buffers.

**Changes in `MidiChannel.cpp`:**
- In `PROC_process_playback()`: when sending stored message to output buffer, set:
  ```cpp
  MidiStorageElem out_event = *event;
  out_event.proc_time = (int)event->storage_time - _pos + buf.first.n_frames_processed;
  ```
- The `storage_time` field in output message should remain 0 (not used for realtime output)
- Alternatively, reuse `proc_time` field by setting it before sending

**End state:** Playback correctly sets `proc_time` based on buffer position.

**Verification:**
- `cargo build` succeeds
- `cargo test` passes
- Playback tests show correct timing

---

**Status:** ✅ Complete — `PROC_process_playback()` already computes `proc_time` correctly:
```cpp
auto proc_time =
    (int)event->storage_time - _pos + buf.first.n_frames_processed;
event->proc_time = proc_time;
```

---

## Phase 4: JACK Ports

### [x] 4.1: Update `jack/JackMidiPort.h`

**Goal:** Remove intermediate buffer classes, implement interfaces directly

**Changes for `GenericJackMidiInputPort`:**
- Remove `MidiBufferingInputPort` inheritance
- Remove `JackMidiReadBuffer` inner class (reference-based)
- Add `std::vector<MidiStorageElem> m_messages` member
- Add `#include "MidiBuffer.h"` (new)
- Implement `MidiReadableBuffer` interface directly:
  - `uint32_t n_events() const`
  - `MidiStorageElem get_event(uint32_t idx) const`
- Remove `PROC_internal_read_input_data_buffer()` override (no longer needed)

**Changes for `GenericJackMidiOutputPort`:**
- Remove `MidiSortingReadWritePort` inheritance
- Remove `JackMidiWriteBuffer` inner class (reference-based)
- Implement `MidiWriteableBuffer` interface directly:
  - `void write_event(MidiStorageElem event)`
- Remove `PROC_internal_write_output_data_to_buffer()` override (no longer needed)
- Add own `MidiSortingBuffer` member for sorting
- Implement `set_muted`, `get_muted`, `set_ringbuffer_n_samples`, `get_ringbuffer_n_samples` methods

**End state:** Self-contained ports, no intermediate buffer classes.

---

### [x] 4.2: Update `jack/JackMidiPort.cpp`

**Goal:** Implement copy-only logic in port classes

**Changes for input port:**
- Remove `JackMidiReadBuffer` implementations
- In `PROC_prepare()`: clear `m_messages` vector
- In `PROC_process()`:
  - Read JACK events directly
  - Copy into `MidiStorageElem`
  - Push to `m_messages` vector
  - Sort `m_messages` by time
- Implement `n_events()`: return `m_messages.size()`
- Implement `get_event(idx)`: return copy of `m_messages[idx]`

**Changes for output port:**
- Remove `JackMidiWriteBuffer` implementations
- Remove `write_by_reference_supported()` and `write_by_value_supported()` checks
- Implement `write_event()`: copy `MidiStorageElem` data to internal sorting buffer
- Implement constructor with sorting buffer initialization
- Sort and write to JACK buffer in `PROC_process()`

**End state:** Copy-only, simplified.

**Verification:**
- `cargo build` succeeds

---

### [x] 4.3: Delete `MidiBufferingInputPort.h` and `.cpp`

**Goal:** Remove intermediate buffer class

**Status:** Already removed via non-inclusion in `JackMidiPort.h`. The files may still exist but are no longer used.

---

### [x] 4.4: Delete `MidiSortingReadWritePort.h`

**Goal:** Remove wrapper class

**Status:** Already removed via non-inclusion in `JackMidiPort.h`. The file may still exist but is no longer used.

---

## Phase 5: Other Ports

### [x] 5.1: Update `DecoupledMidiPort.h/cpp`

**Goal:** Use canonical `MidiStorageElem` instead of custom wrapper

**Changes in `DecoupledMidiPort.h`:**
- Remove `DecoupledMidiMessage` struct (replaced with `MidiStorageElem`)
- Change `Queue` type from `boost::lockfree::spsc_queue<Message>` to `boost::lockfree::spsc_queue<MidiStorageElem>`
- Update `pop_incoming()` and `push_outgoing()` signatures

**Changes in `DecoupledMidiPort.cpp`:**
- In `PROC_process()`: use `get_event()` instead of `PROC_get_event_reference()`
- Remove manual copy: `m.data = std::vector<uint8_t>(size)` → just use the returned `MidiStorageElem`
- In `push_outgoing()`: use `MidiStorageElem` directly

**End state:** Uses canonical message type throughout.

**Verification:**
- `cargo build` succeeds ✅
- `cargo test` passes ✅

---

### [ ] 5.2: Update `lv2/InternalLV2MidiOutputPort.h/cpp`

**Goal:** Remove reference support from LV2 port

**Changes:**
- Remove `PROC_write_event_reference()` method
- Remove `write_by_reference_supported()` and `write_by_value_supported()` methods
- Update `PROC_write_event_value()` → rename to `write_event()`
- Change parameter type to `MidiStorageElem`
- Implement copy logic directly: copy `event.bytes` to LV2 buffer

**End state:** Simplified, copy-only.

**Verification:**
- `cargo build` succeeds

---

## Phase 6: Cleanup

### [ ] 6.1: Delete `MidiBufferInterfaces.h`

**Goal:** Remove old interface definitions

**Changes:**
- Delete `src/backend/internal/MidiBufferInterfaces.h`

**End state:** Old interfaces gone, only new `MidiBuffer.h` remains.

**Verification:**
- `cargo build` succeeds (no references to old file)
- `cargo test` passes

---

### [ ] 6.2: Delete `MidiMessage.h`

**Goal:** Remove obsolete message types

**Changes:**
- Delete `src/backend/internal/MidiMessage.h`
- Delete `src/backend/internal/MidiMessage.cpp`

**End state:** `MidiStorageElem` is the only MIDI message type.

**Verification:**
- `cargo build` succeeds

---

### [ ] 6.3: Delete `DummyMidiBufs.h` and `.cpp`

**Goal:** Remove test helper that only existed for reference interfaces

**Changes:**
- Delete `src/backend/internal/DummyMidiBufs.h`
- Delete `src/backend/internal/DummyMidiBufs.cpp`

**End state:** File removed.

---

### [ ] 6.4: Delete `MidiSortingBuffer.h` and `.cpp`

**Goal:** Remove if no longer needed (or simplify if still used)

**Decision point:**
- If JACK output ports don't need merging anymore → delete
- If still needed → update to new interfaces and keep

**Changes (if deleting):**
- Delete `src/backend/internal/MidiSortingBuffer.h`
- Delete `src/backend/internal/MidiSortingBuffer.cpp`
- Update any references in JACK ports

**End state:** File removed or simplified.

**Verification:**
- `cargo build` succeeds

---

## Phase 7: Tests

### [ ] 7.1: Update test helpers in `src/backend/test/helpers.h`

**Goal:** Use new interfaces in test code

**Changes:**
- Remove implementations of old interfaces
- Implement `MidiReadableBuffer` and `MidiWriteableBuffer` for test fixtures
- Update `stringify_msg()` and `CHECK_MSGS_EQUAL()` to use `MidiStorageElem`

**End state:** Tests use canonical message type.

**Verification:**
- `cargo test` passes

---

### [ ] 7.2: Update `test_MidiChannel.cpp`

**Goal:** Ensure MidiChannel tests work with new interfaces

**Changes:**
- Update message construction to use `MidiStorageElem` directly
- Remove `MidiMessage` usage

**End state:** Tests compile and pass.

**Verification:**
- `cargo test` passes
- `./test_runner` runs

---

### [ ] 7.3: Update `test_MidiStorage.cpp`

**Goal:** Ensure storage tests work with new interfaces

**Changes:**
- Update message construction
- Update message comparison
- Remove any old interface implementations

**End state:** Tests compile and pass.

**Verification:**
- `cargo test` passes

---

### [ ] 7.4: Update `test_MidiRingbuffer.cpp`

**Goal:** Ensure ringbuffer tests work with new interfaces

**Changes:**
- Update message construction
- Update expectations

**End state:** Tests compile and pass.

**Verification:**
- `cargo test` passes

---

### [ ] 7.5: Update `test_JackPorts.cpp`

**Goal:** Ensure JACK port tests work with new interfaces

**Changes:**
- Update message construction for JACK port tests
- Update expectations
- Remove any reference-based interface usage

**End state:** Tests compile and pass.

**Verification:**
- `cargo test` passes
- `./test_runner` runs

---

### [ ] 7.6: Fix time field semantics in tests

**Goal:** Correct tests that incorrectly compare `storage_time` vs `proc_time`

**Background:**
The two time fields have distinct purposes:
- `storage_time`: Position in loop storage (what was recorded, used for retrieval)
- `proc_time`: Position in realtime processing buffer (set at send time)

Tests that push messages into `MidiTestBuffer` for recording should use `proc_time` for input buffer messages. Tests that check output from playback should use `storage_time` for stored messages but `proc_time` for realtime output.

**Changes in `test_AudioMidiLoop_midi.cpp`:**
- `check_msgs_equal()`: Compare `a.proc_time` (from output buffer) against `b.storage_time` (from input/source)
- In recording tests: set `proc_time` on source buffer messages
- In playback tests: check `storage_time` on retrieved contents, check `proc_time` on output messages

**End state:** All tests correctly use the appropriate time field.

**Verification:**
- `cargo test` passes
- `./test_runner` runs - all assertions pass

---

## Phase 8: Final Verification

### [ ] 8.1: Full build

**Goal:** Verify entire project builds

**Command:**
```bash
cargo build
```

**Expected:** Clean build, no errors, no warnings about unused code.

---

### [ ] 8.2: Run cargo test

**Goal:** Verify all unit tests pass

**Command:**
```bash
cargo test
```

**Expected:** All tests pass.

---

### [ ] 8.3: Run test_runner

**Goal:** Verify all integration tests pass

**Command:**
```bash
./target/debug/build/backend-*/out/cmake_build/build/test/test_runner
```

Or find installed location:
```bash
./target/debug/build/backend-*/out/cmake_install/tools/shoopdaloop/test_runner
```

**Expected:** All tests pass.

---

### [ ] 8.4: Verify no remaining references to deleted files

**Goal:** Ensure cleanup is complete

**Commands:**
```bash
grep -r "MidiBufferInterfaces" src/ --include="*.h" --include="*.cpp"
grep -r "MidiMessage" src/ --include="*.h" --include="*.cpp"
grep -r "MidiSortableMessageInterface" src/ --include="*.h" --include="*.cpp"
grep -r "write_by_reference\|read_by_reference" src/ --include="*.h" --include="*.cpp"
```

**Expected:** No matches (or only in documentation/comments).

---

## Summary Checklist

- [x] Phase 1: Foundation (3 tasks) ✅
- [x] Phase 2: Buffer Implementations (4 tasks) ✅
- [x] Phase 3: Message Sending (4 tasks) ⚠️ - 3.4 needs completion
- [ ] Phase 4: JACK Ports (4 tasks)
- [ ] Phase 5: Other Ports (2 tasks)
- [ ] Phase 6: Cleanup (4 tasks)
- [ ] Phase 7: Tests (6 tasks)
- [ ] Phase 8: Final Verification (4 tasks)

**Total: 31 tasks**

---

## Notes

- Tasks can be done in order within phases, but phases should be completed sequentially
- Build and test after each phase to catch issues early
- If a task reveals additional work, create subtasks as needed
- Document any deviations from this plan in the relevant task

### Key Implementation Notes

**Time Field Semantics:**
- `storage_time`: Set once when message is recorded into storage. Never modified after that. Represents message position in the loop.
- `proc_time`: Set each time a message is placed in a realtime processing buffer. Zeroed/irrelevant in storage context.

**Conversion Rules:**
1. Recording: `source_buffer.get_event().proc_time` → `storage.append(..., storage_time = source_buffer_time + record_offset)`
2. Playback: `stored_message.storage_time` → `output_buffer.write_event(proc_time = storage_time - current_pos + buffer_offset)`

**Test Helper (`MidiTestBuffer`):**
- The `read` vector simulates input buffer messages. Use `proc_time` for these messages.
- The `written` vector captures output messages. Use `proc_time` for these (realtime context).
- When comparing recorded contents, use `storage_time`.
- When comparing against source messages, compare `a.proc_time` to `b.storage_time` (or `b.proc_time` depending on direction).