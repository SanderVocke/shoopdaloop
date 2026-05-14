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
- Remove `MidiSortableMessageInterface` inheritance
- Keep existing inline 4-byte storage (`bytes[4]`) — already correct
- Remove `get()` method if no longer needed (or keep for compatibility)
- Ensure `MidiStorageElem` is a plain struct with public data

**End state:** `MidiStorageElem` stands alone, no interface inheritance.

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

### [ ] 2.1: Refactor `MidiSortingBuffer`

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

### [ ] 2.2: Update `MidiPort.h`

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

### [ ] 2.3: Update `MidiPort.cpp`

**Goal:** Implement simplified buffer retrieval

**Changes:**
- Update `PROC_get_*` methods to match new signatures
- Ensure ringbuffer snapshot functionality still works

**End state:** Compiles with new interfaces.

**Verification:**
- `cargo build` succeeds

---

### [ ] 2.4: Update `DummyMidiPort`

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

### [ ] 3.1: Update `MidiChannel.h`

**Goal:** Single send method, remove reference-based overload

**Changes:**
- Remove `PROC_send_message_ref()` declaration
- Keep/rename `PROC_send_message_value()` → `PROC_send_message()`
- Remove `#include "MidiBufferInterfaces.h"` or update
- Update parameter types to use `MidiStorageElem` and `MidiWriteableBuffer`

**End state:** One send method.

---

### [ ] 3.2: Update `MidiChannel.cpp`

**Goal:** Implement single send method, update all call sites

**Changes:**
- Remove `PROC_send_message_ref()` implementation
- Update `PROC_send_message_value()` to be the only send method
- Change signature to take `MidiStorageElem` directly
- Update all internal call sites:
  - Playback logic
  - Sound-off messages
  - State tracking
- Remove checks for `write_by_reference_supported()` and `write_by_value_supported()`
- Update `MidiStateTracker::process_msg()` calls to use `MidiStorageElem::bytes`

**End state:** All message sends go through single method.

**Verification:**
- `cargo build` succeeds
- `cargo test` passes

---

## Phase 4: JACK Ports

### [ ] 4.1: Update `jack/JackMidiPort.h`

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

**End state:** Self-contained ports, no intermediate buffer classes.

---

### [ ] 4.2: Update `jack/JackMidiPort.cpp`

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
- Implement `write_event()`: copy `MidiStorageElem` data to JACK buffer
- Remove sorting logic (can use `MidiSortingBuffer` if needed for merging, or just write in order)

**End state:** Copy-only, simplified.

**Verification:**
- `cargo build` succeeds

---

### [ ] 4.3: Delete `MidiBufferingInputPort.h` and `.cpp`

**Goal:** Remove intermediate buffer class

**Changes:**
- Delete `src/backend/internal/MidiBufferingInputPort.h`
- Delete `src/backend/internal/MidiBufferingInputPort.cpp`

**End state:** File removed.

---

### [ ] 4.4: Delete `MidiSortingReadWritePort.h`

**Goal:** Remove wrapper class

**Changes:**
- Delete `src/backend/internal/MidiSortingReadWritePort.h`

**End state:** File removed.

---

## Phase 5: Other Ports

### [ ] 5.1: Update `DecoupledMidiPort.h/cpp`

**Goal:** Use canonical `MidiStorageElem` instead of custom wrapper

**Changes in `DecoupledMidiPort.h`:**
- Remove `DecoupledMidiMessage` struct (replace with `MidiStorageElem`)
- Change `Queue` type from `boost::lockfree::spsc_queue<Message>` to `boost::lockfree::spsc_queue<MidiStorageElem>`
- Update `pop_incoming()` and `push_outgoing()` signatures

**Changes in `DecoupledMidiPort.cpp`:**
- In `PROC_process()`: use `get_event()` instead of `PROC_get_event_reference()`
- Remove manual copy: `m.data = std::vector<uint8_t>(size)` → just use the returned `MidiStorageElem`
- In `push_outgoing()`: use `MidiStorageElem` directly

**End state:** Uses canonical message type throughout.

**Verification:**
- `cargo build` succeeds

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
- Update any `PROC_send_message_ref()` calls → `PROC_send_message_value()` or new method
- Update message construction to use `MidiStorageElem` directly
- Remove any checks for `write_by_reference_supported()`

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

- [ ] Phase 1: Foundation (3 tasks)
- [ ] Phase 2: Buffer Implementations (4 tasks)
- [ ] Phase 3: Message Sending (2 tasks)
- [ ] Phase 4: JACK Ports (4 tasks)
- [ ] Phase 5: Other Ports (2 tasks)
- [ ] Phase 6: Cleanup (4 tasks)
- [ ] Phase 7: Tests (5 tasks)
- [ ] Phase 8: Final Verification (4 tasks)

**Total: 28 tasks**

---

## Notes

- Tasks can be done in order within phases, but phases should be completed sequentially
- Build and test after each phase to catch issues early
- If a task reveals additional work, create subtasks as needed
- Document any deviations from this plan in the relevant task