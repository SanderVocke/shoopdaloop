# TODO: Rearchitect Rust MIDI State Tracker Port

## Phase 1: Pre-Flight & Discovery

- [x] **1.1 Read current Rust implementation files**
  - **Goal:** Understand the existing code structure in `src/rust/backend_rust/src/midi_state_tracker.rs`, `midi_state_diff_tracker.rs`, `event.rs`, `midi_helpers.rs`, `cxx.rs`, and `lib.rs`.
  - **End condition:** Have a mental map of where channel-based code, `Event` types, `sync()`, `create_subscription()`, and `crossbeam-channel` usages live.

- [x] **1.2 Read original C++ implementation via git**
  - **Goal:** Confirm exact C++ behavior for `MidiStateTracker::subscribe/unsubscribe`, callback invocation order, `MidiStateDiffTracker::reset()` sequence, and destructor cleanup.
  - **End condition:** Have the original C++ code visible for side-by-side comparison during rewrites.

- [x] **1.3 Read C++ wrapper files**
  - **Goal:** Verify that `src/backend/internal/MidiStateTracker.cpp` and `MidiStateDiffTracker.cpp` are thin delegates and confirm no wrapper changes are needed.
  - **End condition:** Confirmed that Rust-side subscription changes are fully internal and C++ wrappers can remain no-ops for subscribe/unsubscribe.

- [x] **1.4 Remove `crossbeam-channel` dependency**
  - **Goal:** Strip the channel dependency from `Cargo.toml`.
  - **End condition:** `cargo build` fails due to missing imports (expected) — confirms dependency is gone.

## Phase 2: Core `Subscriber` Trait & `MidiStateTracker` Rewrite

- [x] **2.1 Define the `Subscriber` trait in `midi_state_tracker.rs`**
  - **Goal:** Create a Rust trait matching C++ `MidiStateTracker::Subscriber` with 5 methods: `note_changed`, `cc_changed`, `program_changed`, `pitch_wheel_changed`, `channel_pressure_changed`.
  - **End condition:** Trait compiles with correct signatures (`&mut self`, `from: &MidiStateTracker`, channel/note, `Option<u8>` / `Option<u16>`).

- [x] **2.2 Replace subscriber storage in `MidiStateTracker`**
  - **Goal:** Remove `subscribers: Vec<Sender<Event>>` and replace with `subscribers: RefCell<Vec<*mut dyn Subscriber>>`.
  - **End condition:** `MidiStateTracker` struct compiles with new subscriber field; no channel types remain in the struct.

- [x] **2.3 Implement `subscribe()` and `unsubscribe()` on `MidiStateTracker`**
  - **Goal:** Add methods to push/remove raw trait object pointers by pointer identity, matching C++ `std::set` insert/erase semantics.
  - **End condition:** `subscribe` adds a `*mut dyn Subscriber`; `unsubscribe` removes all matching pointers; both compile.

- [x] **2.4 Add `get_id()` to `MidiStateTracker` (if not present)**
  - **Goal:** Ensure `MidiStateTracker` exposes a unique `u64` ID so `MidiStateDiffTracker` can identify the sender in callbacks.
  - **End condition:** `get_id()` returns a stable ID; used later in `Subscriber` impl.

- [x] **2.5 Replace channel-based `notify()` with synchronous `notify_*_changed` methods**
  - **Goal:** Remove `notify()` and `create_subscription()`. Add `notify_note_changed`, `notify_cc_changed`, `notify_program_changed`, `notify_pitch_wheel_changed`, `notify_channel_pressure_changed` that iterate `subscribers` and call the trait method directly via `unsafe { (*subscriber).note_changed(...) }`.
  - **End condition:** All 5 notification methods exist and compile; `Event` type is no longer referenced in this file.

- [x] **2.6 Update state-mutating methods to call synchronous notifications**
  - **Goal:** In `process_noteOn`, `process_noteOff`, `process_cc`, `process_program_change`, `process_pitch_wheel`, `process_channel_pressure`, replace any channel-send logic with calls to the new `notify_*_changed` helpers.
  - **End condition:** Every state mutation triggers an inline callback to all subscribers, matching C++ execution order.

## Phase 3: `MidiStateDiffTracker` Rewrite

- [x] **3.1 Define `TrackerRef` wrapper type in `midi_state_diff_tracker.rs`**
  - **Goal:** Create `TrackerRef(Rc<RefCell<Option<Box<MidiStateTracker>>>>)` with an `unsafe fn new(tracker: *mut MidiStateTracker)` and a `with_state<F, R>` accessor.
  - **End condition:** `TrackerRef` compiles and provides safe read-only access to the wrapped tracker; `Box::from_raw` usage is documented with SAFETY comments.

- [x] **3.2 Replace tracker fields in `MidiStateDiffTracker`**
  - **Goal:** Remove any channel/subscription fields (`subscriptions: Vec<Subscription>`, `sync` state). Replace with `tracker_a: Option<TrackerRef>`, `tracker_a_id: u64`, `tracker_b: Option<TrackerRef>`, `tracker_b_id: u64`.
  - **End condition:** `MidiStateDiffTracker` struct compiles with new fields; no `crossbeam_channel` or `Event` types remain.

- [x] **3.3 Implement `Subscriber` for `MidiStateDiffTracker`**
  - **Goal:** Implement all 5 trait methods. Each method: (a) identifies sender vs other tracker by ID, (b) reads live state from both via `TrackerRef::with_state`, (c) compares states, (d) inserts or removes the appropriate diff key in `self.diffs`.
  - **End condition:** All 5 `Subscriber` methods compile and correctly map to diff keys (`0x90`, `0xB0`, etc.).

- [x] **3.4 Rewrite `reset()` with explicit subscribe/unsubscribe lifecycle**
  - **Goal:** Match C++ order exactly: (1) unsubscribe from old `tracker_a`/`tracker_b`, (2) assign new `TrackerRef`s, (3) cache IDs, (4) subscribe to new trackers, (5) perform action (`rescan_diff`/`clear_diff`).
  - **End condition:** `reset()` compiles and uses `self as *mut dyn Subscriber` for subscribe/unsubscribe calls; no old subscriptions leak.

- [x] **3.5 Implement `Drop` for `MidiStateDiffTracker`**
  - **Goal:** On drop, unsubscribe from `tracker_a` and `tracker_b` to ensure the tracker never holds a dangling subscriber pointer.
  - **End condition:** `Drop` impl compiles and mirrors C++ destructor semantics.

- [x] **3.6 Remove `sync()` and all channel-draining code**
  - **Goal:** Delete the `sync()` method and any helpers that processed queued `Event`s. The diff set is now always live.
  - **End condition:** No `sync()` method exists; no event-queue processing code remains.

## Phase 4: Cleanup & Supporting Files

- [x] **4.1 Strip `event.rs`**
  - **Goal:** Remove `Event` enum, `TrackerStateData`, and any channel-related types. If the file becomes empty, remove it from `lib.rs`.
  - **End condition:** `event.rs` contains no MIDI state event types; crate still compiles.

- [x] **4.2 Update `lib.rs` exports**
  - **Goal:** Remove any `pub mod event` or re-exports of channel/Event types. Ensure `midi_state_tracker` (with `Subscriber` trait) and `midi_state_diff_tracker` are correctly exposed.
  - **End condition:** `lib.rs` compiles; module tree is clean.

- [x] **4.3 Verify `cxx.rs` FFI bridge needs no subscription changes**
  - **Goal:** Confirm that `subscribe`/`unsubscribe` are NOT in the FFI bridge and that `reset()` signature still matches C++ wrapper expectations.
  - **End condition:** FFI bridge compiles; no new extern methods needed for subscription.

- [x] **4.4 Fix any remaining compilation errors across the crate**
  - **Goal:** Resolve import errors, missing methods, or type mismatches caused by the removals and rewrites.
  - **End condition:** `cargo build -p backend_rust` succeeds with zero errors.

## Phase 5: Verification & Testing

- [ ] **5.1 Run Rust unit tests**
  - **Goal:** Execute `cargo test -p backend_rust` and fix any failures caused by behavioral changes.
  - **End condition:** All Rust tests pass.

- [ ] **5.2 Run C++ MIDI-related tests via CTest**
  - **Goal:** Execute `cd build && ctest -R Midi --output-on-failure` and identify any remaining divergences.
  - **End condition:** All MIDI-related CTest cases pass.

- [ ] **5.3 Run specific targeted test case**
  - **Goal:** Run `./backend_test -- "[AudioMidiLoop][midi]"` (or equivalent) to verify the most sensitive integration scenario.
  - **End condition:** Targeted test passes.

- [ ] **5.4 Audit for memory leaks / dangling pointer soundness**
  - **Goal:** Review all `unsafe` blocks. Verify: (a) `reset()` always unsubscribes old before subscribing new, (b) `Drop` always unsubscribes, (c) no code path leaves a subscriber pointer in the tracker after the diff tracker is destroyed.
  - **End condition:** Documented SAFETY comments are complete and justify every `unsafe` usage; reviewer (self) is convinced no UB is possible.

- [ ] **5.5 Final diff comparison check (optional but recommended)**
  - **Goal:** For a representative sequence of MIDI messages, manually trace both C++ and Rust behavior to confirm diff sets are identical after every message.
  - **End condition:** Confident that the Rust implementation is behaviorally equivalent to the C++ original.

## Phase 6: Documentation & Wrap-up

- [ ] **6.1 Update inline comments documenting the C++-matching design**
  - **Goal:** Add doc comments on `Subscriber`, `TrackerRef`, `subscribe/unsubscribe`, and `reset()` explaining how they mirror the original C++ architecture.
  - **End condition:** Future maintainers can read the Rust code and understand the intentional C++ parity.

- [ ] **6.2 Delete or archive any temporary reference files**
  - **Goal:** Remove `MidiStateTracker.original.h` or any other git-dumped scratch files if they are no longer needed.
  - **End condition:** Working tree is clean of temporary artifacts.

- [ ] **6.3 Final full test suite run**
  - **Goal:** Run the broadest test command available (`cargo test`, `ctest`, or project-specific full suite) to ensure no regressions.
  - **End condition:** Full suite passes; rearchitecture is complete.
