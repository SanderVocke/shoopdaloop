# Test Changes for Single `time` Field

This document catalogs all test occurrences of `proc_time` and `storage_time`, and describes how each should be updated to work with a single `time` field.

## Core Principle

With a single `time` field:
- **Input buffers** (source messages being recorded): `time` = relative position within the input buffer
- **Storage**: `time` = absolute position in the loop
- **Output buffers** (written messages): `time` = relative position within the output buffer (computed at send time)
- **API retrieval**: `time` = absolute storage position

When comparing messages across these boundaries, the test must account for the time conversion that happens at the boundary.

---

## helpers.h Changes

### `create_note_msg`, `create_noteOn`, `create_noteOff`, `create_cc` (lines 75, 89, 99, 115, 126)

```cpp
// Current:
rval.proc_time = time;

// After:
rval.time = time;
```

**Reason:** Helper functions create messages for input buffers (source). These use `proc_time` but should use `time`.

---

### `convert_midi_msgs_to_api` (line 136, 142)

```cpp
// Current:
sequence->length_samples = msgs.back().proc_time + 1;
event.time = msg.proc_time;

// After:
sequence->length_samples = msgs.back().time + 1;
event.time = msg.time;
```

**Reason:** When converting to API sequence, `time` is the source message's time (relative for input, absolute for storage).

---

### `convert_api_midi_msgs` (line 167)

```cpp
// Current:
m.proc_time = ev->time;

// After:
m.time = ev->time;
```

**Reason:** When converting from API, `time` should be set from the sequence event.

---

### `stringify_msg` (line 182)

```cpp
// Current:
s << "{ t=" << m.proc_time << ", s=" << m.size << ", d={";

// After:
s << "{ t=" << m.time << ", s=" << m.size << ", d={";
```

**Reason:** Print the single `time` field.

---

### Catch StringMaker for MidiStorageElem (line 214)

```cpp
// Current:
oss << "{ t:" << e.proc_time << ", s:" << e.size << ", d:[";

// After:
oss << "{ t:" << e.time << ", s:" << e.size << ", d:[";
```

**Reason:** Print the single `time` field.

---

## test_MidiChannel.cpp

### Set Contents test (lines 17-19)

```cpp
// Current:
msg.storage_time = 0; msg.size = 3; msg.bytes[0] = 0; msg.bytes[1] = 1; msg.bytes[2] = 2; data.push_back(msg);
msg.storage_time = 1; msg.size = 3; msg.bytes[0] = 3; msg.bytes[1] = 4; msg.bytes[2] = 5; data.push_back(msg);
msg.storage_time = 10; msg.size = 1; msg.bytes[0] = 10; msg.bytes[1] = 0; msg.bytes[2] = 0; data.push_back(msg);

// After:
msg.time = 0; msg.size = 3; msg.bytes[0] = 0; msg.bytes[1] = 1; msg.bytes[2] = 2; data.push_back(msg);
msg.time = 1; msg.size = 3; msg.bytes[0] = 3; msg.bytes[1] = 4; msg.bytes[2] = 5; data.push_back(msg);
msg.time = 10; msg.size = 1; msg.bytes[0] = 10; msg.bytes[1] = 0; msg.bytes[2] = 0; data.push_back(msg);
```

**Reason:** These messages are being set into storage, so `time` represents absolute storage positions.

---

## test_MidiStorage.cpp

### StringMaker for MidiStorageElem (line 29)

```cpp
// Current:
oss << "{ t:" << e.storage_time << ", s:" << e.size << ", d:";

// After:
oss << "{ t:" << e.time << ", s:" << e.size << ", d:";
```

**Reason:** Print the single `time` field.

---

### `make_msg` helper (line 56)

```cpp
// Current:
msg.storage_time = time;

// After:
msg.time = time;
```

**Reason:** The helper creates messages for input to storage.

---

### All `append` calls (lines 74, 117, 118, 164, 165)

```cpp
// Current:
s->append(i.storage_time, i.size, i.bytes);
s->append(append.storage_time, append.size, append.bytes);

// After:
s->append(i.time, i.size, i.bytes);
s->append(append.time, append.size, append.bytes);
```

**Reason:** `MidiStorage::append` takes `time` parameter (was `storage_time`).

---

### Cursor reading (lines 85, 129, 176, 222)

```cpp
// Current:
msg.storage_time = elem->storage_time;

// After:
msg.time = elem->time;
```

**Reason:** When reading from storage cursor, `elem->time` is the storage time.

---

## test_MidiRingbuffer.cpp

### StringMaker for MidiStorageElem (line 34)

```cpp
// Current:
oss << "{ t:" << e.storage_time << ", s:" << e.size << ", d:";

// After:
oss << "{ t:" << e.time << ", s:" << e.size << ", d:";
```

**Reason:** Print the single `time` field.

---

### Cursor reading (line 66)

```cpp
// Current:
msg.storage_time = elem->storage_time;

// After:
msg.time = elem->time;
```

**Reason:** When reading from ringbuffer cursor.

---

### `make_msg` helper (line 78)

```cpp
// Current:
msg.storage_time = time;

// After:
msg.time = time;
```

**Reason:** Helper creates messages for ringbuffer input.

---

### All `put` calls (lines 94, 98, 103, 108, 133, 137, 142, 146, 151, 175, 179, 183, 188, 193, 218, 222, 226, 231, 236, 271, 310)

```cpp
// Current:
CHECK(b->put(m.storage_time, m.size, m.bytes) == true);

// After:
CHECK(b->put(m.time, m.size, m.bytes) == true);
```

**Reason:** `MidiRingbuffer::put` takes `time` parameter (was `storage_time`).

---

## test_JackPorts.cpp

### Line 24 - `proc_time` from m.time

```cpp
// Current:
msg.proc_time = m.time;

// After:
msg.time = m.time;
```

**Reason:** Set message time from JACK event time.

---

### Lines 264-265, 288-289, 305-306, 454-455, 478-480, 501-502, 606-607 - MidiStorageElem initialization

```cpp
// Current:
MidiStorageElem m1; m1.proc_time = 0; m1.size = 3; ...
MidiStorageElem m2; m2.proc_time = 0; m2.size = 3; ...

// After:
MidiStorageElem m1; m1.time = 0; m1.size = 3; ...
MidiStorageElem m2; m2.time = 0; m2.size = 3; ...

// Note: Messages at time 0 are used as input buffers (relative time).
// Messages at time 10+ are used for sorting buffer testing (relative time).
```

**Reason:** All test messages use the single `time` field.

---

### Line 421 - Cursor reading

```cpp
// Current:
msg.storage_time = elem->storage_time;

// After:
msg.time = elem->time;
```

**Reason:** When reading from storage/buffer cursor.

---

## test_AudioMidiLoop_midi.cpp

This is the largest file with the most changes. Key patterns:

### `check_msgs_equal` helper (line 42)

```cpp
// Current:
CHECK(a.storage_time == (uint32_t)b.proc_time+time_offset);

// After:
// When comparing recorded (storage) to source (input buffer):
// - Recorded: a.time is absolute storage position
// - Source: b.time is relative to source buffer
// The offset accounts for recording start position
CHECK(a.time == b.time + recording_start_position + time_offset);
```

**Important:** This comparison always happens between messages from storage (absolute) and messages from source buffer (relative). The test must know the recording start position to compute the expected storage time.

---

### `with_time` helper (line 66)

```cpp
// Current:
Message m = msg;
m.storage_time = time;
return m;

// After:
Message m = msg;
m.time = time;
return m;
```

**Reason:** Set the single `time` field.

---

### Lines 82, 281, 293-297, 350-352, 477, 526 - Storage message creation

```cpp
// Current:
_m.storage_time = time;
msg.storage_time = 0; msg.size = 1; msg.bytes[0] = 0x01; contents.recorded_msgs.push_back(msg);
msg.storage_time = 10; ...

// After:
_m.time = time;
msg.time = 0; msg.size = 1; msg.bytes[0] = 0x01; contents.recorded_msgs.push_back(msg);
msg.time = 10; ...
```

**Reason:** Messages being set into storage use `time` as absolute position.

---

### Lines 132-134, 170-172, 206-215, 309-311, 403-406 - Source buffer messages (input)

```cpp
// Current:
source_buf.read.push_back(Msg()); source_buf.read.back().proc_time = 10; ...

// After:
source_buf.read.push_back(Msg()); source_buf.read.back().time = 10; ...
```

**Reason:** Source buffer messages use `time` as relative position within the input buffer. This is correct semantics.

---

### Lines 872-917, 1016-1192 - Checking output buffer `storage_time`

```cpp
// Current:
CHECK(msgs[0].storage_time == 0);
CHECK((int)msgs[1].storage_time == 8);

// After:
// The output buffer contains messages written during playback.
// These messages have `time` set to their relative position in the output buffer.
// When checking output, we should verify the RELATIVE time, not storage time.

// For playback output tests:
// msgs[i].time == expected_relative_position

// Example transformation:
CHECK(msgs[0].time == 0);    // Written at start of buffer
CHECK(msgs[1].time == 8);    // etc.
```

**Important:** These tests currently check `storage_time` on output buffer messages, but output buffers should contain relative times. The tests need to be rewritten to:
1. Track expected relative times in the output buffer
2. Verify `msg.time` matches those expectations

---

### Multiple source buffer tests (lines 206-215)

These tests set up multiple recording sessions with offset buffers:

```cpp
// Current (buffer 1 at times 0, 5, 8 relative, which is 21, 26, 29 absolute):
source_bufs[1].read.push_back(Msg()); source_bufs[1].read.back().proc_time = 21-21; ...
source_bufs[1].read.push_back(Msg()); source_bufs[1].read.back().proc_time = 26-21; ...
source_bufs[1].read.push_back(Msg()); source_bufs[1].read.back().proc_time = 29-21; ...

// After:
source_bufs[1].read.push_back(Msg()); source_bufs[1].read.back().time = 21-21; ...
source_bufs[1].read.push_back(Msg()); source_bufs[1].read.back().time = 26-21; ...
source_bufs[1].read.push_back(Msg()); source_bufs[1].read.back().time = 29-21; ...
```

**Note:** The values `21-21=0`, `26-21=5`, `29-21=8` are the relative times within the second buffer. This is correct — the buffer is pre-shifted to account for the recording position.

---

## test_chain_single_direct_loop.cpp (integration tests)

### Line 151 - Storage message creation

```cpp
// Current:
_m.storage_time = time;

// After:
_m.time = time;
```

---

### Lines 220, 252, 299, 376, 418, 445, 564, 642, 722 - Source message queuing

```cpp
// Current:
tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.proc_time, msg.bytes);

// After:
tst.int_dummy_midi_input_port->queue_msg(msg.size, msg.time, msg.bytes);
```

**Reason:** `queue_msg` takes a relative time (within the queued buffer), which becomes `time` in the message.

---

## Summary of Changes by File

| File | Changes |
|------|---------|
| `helpers.h` | 8 occurrences: helper functions, stringify, StringMaker |
| `test_MidiChannel.cpp` | 3 occurrences: storage message creation |
| `test_MidiStorage.cpp` | 10 occurrences: StringMaker, make_msg, append calls, cursor reading |
| `test_MidiRingbuffer.cpp` | 28 occurrences: StringMaker, make_msg, all put calls, cursor reading |
| `test_JackPorts.cpp` | 18 occurrences: MidiStorageElem initialization, cursor reading |
| `test_AudioMidiLoop_midi.cpp` | ~80 occurrences: source buffers, storage messages, check functions, output assertions |
| `test_chain_single_direct_loop.cpp` | 10 occurrences: storage creation, queue_msg calls |

---

## Test Logic That Needs Special Attention

### 1. `check_msgs_equal` in test_AudioMidiLoop_midi.cpp

This function compares recorded messages (absolute time) against source buffer messages (relative time). The current signature:
```cpp
check_msgs_equal(a, b, time_offset=0)
```

New approach: The test should explicitly track what the expected storage time should be:
```cpp
// Option A: Update check_msgs_equal to compute expected storage time
uint32_t expected_storage_time = b.time + recording_start_position + time_offset;
CHECK(a.time == expected_storage_time);

// Option B: Change callers to pre-compute expected storage time
MidiStorageElem expected = source_msg;
expected.time = source_msg.time + recording_start_position;
check_msgs_equal(recorded_msg, expected);
```

### 2. Output buffer assertions

Tests that check `msgs[n].storage_time` on output buffers need to be rewritten to check relative times:

```cpp
// Current:
CHECK(msgs[0].storage_time == 0);

// After:
CHECK(msgs[0].time == 0);  // Relative to output buffer start

// Or if the test needs to verify message positions in storage:
// Store the expected storage times separately and compare
std::vector<uint32_t> expected_storage_times = {0, 8, 18, 28};
for (size_t i = 0; i < msgs.size(); i++) {
    // The message's time in the output buffer
    CHECK(msgs[i].time == expected_output_buffer_times[i]);
}
```

### 3. Multiple buffer recording tests

Tests like "Record multiple source buffers" set up pre-shifted times in source buffers. The logic remains the same, just rename `proc_time` to `time`:
```cpp
// Buffer 2 starts recording at position 30, messages at times 0, 1, 2, 11 relative
// are stored at 30, 31, 32, 41 absolute
source_bufs[2].read.back().time = 30-21-9;  // = 0 relative in this buffer
```

---

## Verification Checklist

After making changes:
- [ ] All `proc_time` references renamed to `time`
- [ ] All `storage_time` references renamed to `time`
- [ ] `check_msgs_equal` logic verified for correct absolute/relative time comparison
- [ ] Output buffer assertions verify relative times, not absolute
- [ ] `stringify_msg` and StringMaker print `time` field
- [ ] All tests pass