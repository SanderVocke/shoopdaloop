# Refactor MidiStorage to fixed 4-byte messages

## Goal
Disregard MIDI SysEx and assume every stored message fits in 4 bytes. This makes every `MidiStorageElem` the same size, allowing us to replace the raw-byte ringbuffer with a typed circular array (`std::vector<MidiStorageElem>`) and remove all byte-offset / variable-size / filler complexity.

## Assumptions
- No message will ever need > 4 bytes of payload.
- `MidiStorageElem::size` can still be 1..4 (useful for state trackers, channel pressure, program change), but the *struct* always allocates 4 bytes inline.
- 4 bytes also covers MIDI 2.0 UMP (Universal MIDI Packet), making the storage future-proof for modern MIDI APIs.

---

## Chunk 1: Fix the element shape
**Files:** `src/backend/internal/MidiStorage.h`, `src/backend/internal/MidiStorage.cpp`

- [x] Change `MidiStorageElem`:
  - Replace the trailing implicit data bytes with `uint8_t data[4];`.
  - Remove `#pragma pack(push, 1)` and `#pragma pack(pop)`.
  - Remove `static uint32_t total_size_of(uint32_t data_bytes)`.
  - Remove `uint16_t offset_to_next`.
  - Change `data()` / `get_data()` to return `data` directly.
  - Keep `uint16_t size` (still meaningful for 1-byte vs 4-byte messages).

- [x] Update `MidiStorageBase` helpers that touch the element:
  - Remove `store_unsafe(uint32_t offset, ..., uint16_t n_filler)`. Replace with a simple `store_at(uint32_t elem_idx, uint32_t t, uint16_t s, const uint8_t* d)` that assigns the struct.
  - Remove `unsafe_at(uint32_t offset)`. We will later remove it entirely, but for now change it to index by `sizeof(MidiStorageElem)`.
  - Keep `offset_to_next` logic temporarily by always setting it to `sizeof(MidiStorageElem)` wherever it is written.

- [x] Update consumers of `total_size_of`:
  - `MidiChannel.cpp` (`set_contents`): replace `MidiStorageElem::total_size_of(elem.data.size())` with `sizeof(MidiStorageElem)`.
  - `MidiStorage` ctor / capacity queries: keep byte semantics for now but use `sizeof(MidiStorageElem)`.

**Verification:** Build passes. `test_MidiStorage` round-trip and prepend tests should still pass (they use 1-byte and 3-byte messages). Replace-append tests will still pass because messages fit.

---

## Chunk 2: Convert `MidiStorageBase` from byte vector to element circular array
**Files:** `src/backend/internal/MidiStorage.h`, `src/backend/internal/MidiStorage.cpp`

- [x] Change backing store:
  - Replace `std::vector<uint8_t> m_data` with `std::vector<MidiStorageElem> m_data`.
  - The ctor argument `uint32_t data_size` should now be interpreted as a **byte budget** rounded down to `data_size / sizeof(MidiStorageElem)` elements. Keep the public signature for now to avoid breaking callers.

- [x] Convert offsets to element indices:
  - `m_head`, `m_tail` become element indices (`uint32_t`).
  - Remove `m_head_start` entirely.
  - Remove `valid_elem_at(uint32_t offset)`.
  - Remove `maybe_next_elem_offset(Elem*)`.
  - Remove `unsafe_at()`.
  - Remove `bytes_occupied()` / `bytes_free()` branching logic; replace with:
    ```cpp
    uint32_t n_events() const { return m_n_events; }
    uint32_t capacity_elems() const { return m_data.size(); }
    bool full() const { return m_n_events == m_data.size(); }
    ```
  - Keep `bytes_capacity()` for external callers (returns `m_data.size() * sizeof(Elem)`).

- [x] Rewrite `append()`:
  - No more byte-size calculations, filler, or boundary-crossing loops.
  - If full and `allow_replace == false`, return false.
  - If full and `allow_replace == true`, overwrite oldest: advance `m_tail` and call `dropped_msg_cb`.
  - Assign element at `m_head`, advance `m_head` modulo capacity, increment `m_n_events` (capped at capacity).
  - Still reject out-of-order messages.

- [x] Rewrite `prepend()`:
  - Symmetric to append but backward. No filler needed.
  - Reject if full or out-of-order.

- [x] Rewrite `copy()`:
  - Simple iteration from tail to head assigning into `to.m_data` contiguously.
  - Set `to.m_tail = 0`, `to.m_head = m_n_events`, `to.m_n_events = m_n_events`.

- [x] Rewrite `clear()`:
  - `m_head = m_tail = m_n_events = 0`.

**Verification:** `test_MidiStorage` should pass except tests that explicitly exercise variable-size messages (>4 bytes) or byte-level wrap-around filler. Those tests will be updated in Chunk 7.

---

## Chunk 3: Simplify `MidiStorageCursor`
**Files:** `src/backend/internal/MidiStorage.h`, `src/backend/internal/MidiStorage.cpp`

- [x] Change cursor state:
  - `m_offset` and `m_prev_offset` become `std::optional<uint32_t>` element indices (same type, new semantics).
  - No more byte arithmetic.

- [x] Rewrite navigation:
  - `reset()`: `m_offset = m_storage->m_tail`, `m_prev_offset = nullopt`.
  - `next()`: `m_offset = (m_offset + 1) % capacity` (or invalidate if we reached head).
  - `get()`: return `m_offset.has_value() ? &m_storage->m_data[m_offset.value()] : nullptr`.
  - `get_prev()`: same with `m_prev_offset`.
  - Remove `wrapped()` or implement it trivially (`m_prev_offset > m_offset`).

- [x] Rewrite find functions:
  - `find_fn_forward()`: simple `for` loop over element indices from current to head.
  - `find_time_forward()`: thin wrapper over `find_fn_forward`.
  - No more `maybe_next_elem_offset` calls.

**Verification:** `test_MidiStorage` round-trip, prepend, replace-append tests pass.

---

## Chunk 4: Simplify `MidiStorage` truncation and iteration
**Files:** `src/backend/internal/MidiStorage.h`, `src/backend/internal/MidiStorage.cpp`

- [x] Rewrite `truncate_fn()`:
  - Head truncate: walk from tail forward, count elements to keep. Set `m_head = keep_count`, `m_n_events = keep_count`.
  - Tail truncate: find first element to keep. Set `m_tail = first_keep_idx`, `m_n_events -= dropped_count`.
  - Invalidate cursors that fall outside the new [tail, head) range.
  - Remove the temporary cursor creation; operate directly on indices.

- [x] Rewrite `truncate()`:
  - Delegate to `truncate_fn` with time predicate (unchanged logic, simpler implementation).

- [x] Rewrite `for_each_msg()` / `for_each_msg_modify()`:
  - Simple index loop from `m_tail` for `m_n_events` steps.
  - Remove the cursor allocation and the "looped back to start" check.

- [x] Update cursor invalidation in `append()`:
  - When overwriting the oldest element (tail advance), any cursor whose `m_offset == old_tail` should be `reset()` or `invalidate()`.
  - This is simpler now because we compare element indices.

**Verification:** `test_MidiStorage` truncation tests. `test_MidiRingbuffer` truncate tests.

---

## Chunk 5: Update `MidiRingbuffer`
**Files:** `src/backend/internal/MidiRingbuffer.h`, `src/backend/internal/MidiRingbuffer.cpp`

- [x] Adapt to new `MidiStorage` API:
  - `put()` still delegates to `append(..., true, dropped_cb)`. Should work without changes except `bytes_capacity()` semantics.
  - `next_buffer()`: the timestamp overflow shift still iterates over all messages; use `for_each_msg_modify()`.
  - `snapshot()`: still copies then truncates. Should work.
  - `set_n_samples()`: truncate call is unchanged.

- [x] Update capacity / sizing:
  - Ensure `MidiRingbuffer` ctor budget is still honored (the base class now rounds bytes down to elements).

**Verification:** `test_MidiRingbuffer` put-and-increment, put-and-truncate tests.

---

## Chunk 6: Update `MidiChannel` and other non-test consumers
**Files:** `src/backend/internal/MidiChannel.cpp`, `src/backend/internal/MidiChannel.h`

- [x] `MidiChannel::set_contents()`:
  - Remove the `min_storage_size` loop that sums `total_size_of`.
  - Replace with:
    ```cpp
    size_t min_storage_size = contents.recorded_msgs.size() * sizeof(MidiStorageElem);
    size_t new_storage_size = std::max((size_t)mp_storage->bytes_capacity(), min_storage_size);
    auto s = shoop_make_shared<Storage>(new_storage_size);
    ```
  - The append loop inside `fn` is unchanged (still calls `s->append(time, size, data)`).

- [x] Check `MidiPort.cpp`:
  - `PROC_snapshot_ringbuffer_into()` uses `m_midi_ringbuffer->n_events()` and `snapshot()`. No changes needed.

- [x] Check `GraphMidiPort.cpp`:
  - No direct `MidiStorageElem` usage.

**Verification:** Build passes. `test_MidiChannel` passes.

---

## Chunk 7: Update / prune tests
**Files:** `src/backend/test/unit/test_MidiStorage.cpp`, `src/backend/test/unit/test_MidiRingbuffer.cpp`

- [x] `test_MidiStorage`:
  - Remove all uses of `Storage::Elem::total_size_of(...)`. Replace with `sizeof(Storage::Elem)`.
  - Remove tests that append messages with `size > 4` (they are invalid under the new assumption).
  - Remove assertions on `bytes_occupied() == bytes_capacity()` when using variable sizes; replace with `n_events() == expected_count`.
  - `extract_messages()` helper still works because `elem->data()` and `elem->size` still exist.

- [x] `test_MidiRingbuffer`:
  - Remove tests that exercise **filler** logic (`Put and wrap with filler`, `Put and wrap with filler big msg`). Filler no longer exists.
  - Remove tests that construct messages > 4 bytes.
  - Remove `constexpr size_t three_byte_msg_size = elem_size + 3` math. Buffer sizes can be expressed as `N * sizeof(Elem)`.
  - Keep tests for wrap-around (it still happens at the element level), but remove the byte-level edge cases.
  - `extract_messages()` helper still works.

- [x] Add new tests (optional but recommended):
  - Fill buffer exactly to capacity and confirm overwrite behavior.
  - Wrap-around with index boundary (capacity = 3, append 4 messages, verify oldest is dropped).

**Verification:** All MidiStorage and MidiRingbuffer tests pass.

---

## Chunk 8: Final cleanup
**Files:** `src/backend/internal/MidiStorage.h`, `src/backend/internal/MidiStorage.cpp`, and any others.

- [x] Remove dead code:
  - `store_unsafe` (if any variant remains).
  - `valid_elem_at`.
  - `maybe_next_elem_offset`.
  - `unsafe_at`.
  - `m_head_start`.
  - `#pragma pack` (already removed in chunk 1, verify).
  - `bytes_occupied()` / `bytes_free()` if no callers remain (check first; `MidiRingbuffer` and tests may use them).
  - `offset_to_next` field on `MidiStorageElem` (should be gone since chunk 2).

- [x] Remove stale log messages:
  - Any log line mentioning "filler" in `MidiStorage.cpp`.

- [x] Update includes:
  - Remove `<cstring>` from `MidiStorage.cpp` if `memcpy` is no longer used for element writes.

- [x] Verify no other files reference removed symbols:
  - Run: `grep -rn "total_size_of\|offset_to_next\|store_unsafe\|valid_elem_at\|maybe_next_elem_offset\|unsafe_at\|n_filler" src/backend/`

**Verification:** Clean build, zero grep hits for removed symbols, all tests pass.

---

## Acceptance Criteria
- [x] `sizeof(MidiStorageElem)` is the only size ever used for storage arithmetic.
- [x] `std::vector<MidiStorageElem>` backs the storage.
- [x] No byte-offset pointer arithmetic remains in `MidiStorage`, `MidiStorageCursor`, or `MidiRingbuffer`.
- [x] All backend tests pass.
- [x] No references to `total_size_of`, `offset_to_next`, `store_unsafe`, `valid_elem_at`, `maybe_next_elem_offset`, `unsafe_at`, or `n_filler` remain in the codebase.
