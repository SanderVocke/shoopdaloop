# Audio Port C++ Refactoring

Refactor the Audio port class hierarchy to match the MIDI port restructuring pattern (see `todo.md`). Extract base classes and interfaces to prepare for Rust trait-based composition.

**Build/Test Reference:** See `build_instructions.md`:
- Full build: `cargo build` (includes Rust + C++)
- Unit tests: `cargo build` produces `test_runner`, run from `target/debug/test_runner`
- Integration tests: `./target/debug/shoopdaloop_dev.sh --self-test`

---

## Overview

### Current Architecture
```
PortInterface (pure virtual)
    └── AudioPort<SampleT> (template - all logic here)
            ├── GraphAudioPort (contains shoop_shared_ptr<AudioPort>)
            ├── DummyAudioPort (inherits AudioPort + DummyPort + WithCommandQueue)
            └── InternalAudioPort (inherits AudioPort)
```

### Target Architecture
```
PortInterface (pure virtual)
    └── AudioPortBase (non-template - core logic holder)
            └── AudioPort<SampleT> (template - contains AudioPortBase, delegates)
                    ├── GraphAudioPort (contains shoop_shared_ptr<AudioPort>)
                    ├── DummyAudioPort (inherits AudioPort)
                    └── InternalAudioPort (inherits AudioPort)
```

### Key Findings
- Only `AudioPort<float>` is actively used in the codebase
- `AudioPort<int>` has `extern template` but **zero instantiations**
- JACK backend uses `jack_default_audio_sample_t` which maps to `float` on Linux
- Therefore: Create concrete non-template `AudioPortBase` for `float` only

### Architecture Decision
- **AudioPortBase** will be a non-template class for `float` samples
- **AudioPort<float>** (the only instantiation) will delegate to `AudioPortBase`
- This matches the MIDI pattern: `MidiPortBase` (concrete) → `MidiPort`
- If `int` support ever needed, create separate `AudioPortBaseInt`

---

## Phase 1: Extract Audio Buffer Interfaces

### 1.1 Create `IAudioReadableBuffer.h`

Pure virtual interface for reading audio data.

- [x] Create `src/backend/internal/IAudioReadableBuffer.h`
- [x] Define methods: `audio_sample_t* get_read_ptr()`, `uint32_t n_samples()`, `void get_peak(float& in_peak, float& out_peak)`
- [x] Document interface with docstrings

### 1.2 Create `IAudioWriteableBuffer.h`

Pure virtual interface for writing audio data.

- [x] Create `src/backend/internal/IAudioWriteableBuffer.h`
- [x] Define methods: `audio_sample_t* get_write_ptr()`, `uint32_t capacity()`, `void set_gain(float)`
- [x] Document interface with docstrings

### 1.3 Update `AudioPort.h` to implement interfaces

- [x] Make `AudioPort<SampleT>` implement `IAudioReadableBuffer` and `IAudioWriteableBuffer`
- [x] Add `get_readable_buffer()` returning `IAudioReadableBuffer*`
- [x] Add `get_writeable_buffer()` returning `IAudioWriteableBuffer*`
- [x] Update `DummyAudioPort` to delegate buffer methods

### 1.4 Verify compilation and tests

- [x] **Build:** `cargo build` succeeds
- [x] **Tests:** `target/debug/test_runner` passes
- [x] **Integration:** `shoopdaloop_dev.sh --self-test` passes

---

## Phase 2: Extract Audio Trait Interfaces

### 2.1 Create `IAudioStateTracking.h`

Interface for audio state (peak, gain, mute).

- [x] Create `src/backend/internal/IAudioStateTracking.h`
- [x] Define methods:
  - `float get_input_peak() const`
  - `void reset_input_peak()`
  - `float get_output_peak() const`
  - `void reset_output_peak()`
  - `float get_gain() const`
  - `void set_gain(float)`
  - `bool get_muted() const`
  - `void set_muted(bool)`
- [x] Update `AudioPort<SampleT>` to implement this interface
- [x] Document interface with docstrings

### 2.2 Create `IAudioRingbuffer.h`

Interface for audio ringbuffer operations.

- [x] Create `src/backend/internal/IAudioRingbuffer.h`
- [x] Define methods:
  - `void set_ringbuffer_n_samples(unsigned n)`
  - `unsigned get_ringbuffer_n_samples() const`
  - `BufferQueue<SampleT>::Snapshot get_ringbuffer_contents()`
- [x] Update `AudioPort<SampleT>` to implement this interface
- [x] Document interface with docstrings

### 2.3 Verify compilation and tests

- [x] **Build:** `cargo build` succeeds
- [x] **Tests:** `target/debug/test_runner` passes

---

## Phase 3: Create AudioPortBase

Create a non-template base class to hold core audio port logic.

### 3.1 Create `AudioPortBase.h`


- [x] Create `src/backend/internal/AudioPortBase.h`
- [x] Non-template class for `float` samples only
- [x] Contains atomic state:
  - `std::atomic<float> m_input_peak`
  - `std::atomic<float> m_output_peak`
  - `std::atomic<float> m_gain`
  - `std::atomic<bool> m_muted`
- [x] Contains `BufferQueue<float> mp_always_record_ringbuffer`
- [x] Implement `IAudioStateTracking` interface
- [x] Implement `IAudioRingbuffer` interface
- [x] Add methods: `process_samples()`, `get_ringbuffer_contents()`
- [x] Document: "For float samples only - if int support needed, create AudioPortBaseInt"

### 3.2 Create `AudioPortBase.cpp`

- [x] Create `src/backend/internal/AudioPortBase.cpp`
- [x] Implement all methods from header
- [x] Implement `BufferQueue<float>` ringbuffer integration
- [x] Implement sample processing (peak tracking, gain, mute)

### 3.4 Verify compilation

- [x] **Build:** `cargo build` succeeds
- [x] **Tests:** `target/debug/test_runner` passes

---

## Phase 4: Update AudioPort with Composition

Update `AudioPort<SampleT>` to use composition.

### 4.1 Update `AudioPort.h`

- [ ] Change `AudioPort<float>` (primary instantiation) to inherit from `AudioPortBase`
- [ ] Remove duplicate atomic members (now in AudioPortBase)
- [ ] Delegate `IAudioStateTracking` methods to AudioPortBase
- [ ] Delegate `IAudioRingbuffer` methods to AudioPortBase
- [ ] Keep template-specific: `PROC_get_buffer()`, `UsedBufferPool`
- [ ] `AudioPort<int>` remains separate (rarely used) - can delegate to AudioPortBaseFloat if needed
- [ ] Maintain full backward compatibility

**Note:** Only `AudioPort<float>` will use full delegation. Other types are optional.

### 4.2 Update `AudioPort.cpp`

- [ ] Remove atomic member initializations from `AudioPort<float>` constructor
- [ ] Delegate all state/gain/mute methods to AudioPortBase
- [ ] Keep sample-processing in template (delegates to base for peak/gain/mute)
- [ ] Handle BufferQueue integration via AudioPortBase

### 4.3 Verify compilation and tests

- [ ] **Build:** `cargo build` succeeds
- [ ] **Tests:** `target/debug/test_runner` passes
- [ ] **Integration:** `shoopdaloop_dev.sh --self-test` passes

---

## Phase 5: Update Audio Port Inheritors

### 5.1 Update `DummyAudioPort`

- [ ] Verify inheritance from `AudioPort<audio_sample_t>` still works
- [ ] Verify delegation to AudioPortBase through AudioPort
- [ ] Check queue/data operations still work
- [ ] **Tests:** `target/debug/test_runner` passes

### 5.2 Update `InternalAudioPort`

- [ ] Verify inheritance from `AudioPort<SampleT>` still works
- [ ] Verify delegation to AudioPortBase through AudioPort
- [ ] **Tests:** `target/debug/test_runner` passes

### 5.3 Update `GraphAudioPort`

- [ ] Verify contains `shoop_shared_ptr<AudioPort<audio_sample_t>>` (composition pattern already in place)
- [ ] No structural changes needed - architecture already compliant
- [ ] **Tests:** `target/debug/test_runner` passes

### 5.4 Verify integration tests

- [ ] **Integration:** `shoopdaloop_dev.sh --self-test` passes

---

## Phase 6: Test Infrastructure Refactoring

### 6.1 Update test expectations

- [ ] Review audio-related test cases
- [ ] Verify audio port tests pass with new composition model
- [ ] Add tests for new interfaces if needed

### 6.2 Verify test coverage

- [ ] **Build:** `cargo build` succeeds
- [ ] **Tests:** `target/debug/test_runner` passes (all assertions)
- [ ] **Integration:** `shoopdaloop_dev.sh --self-test` passes

---

## Phase 7: API Documentation

### 7.1 Document interfaces

- [ ] Add docstrings to `IAudioReadableBuffer.h`
- [ ] Add docstrings to `IAudioWriteableBuffer.h`
- [ ] Add docstrings to `IAudioStateTracking.h`
- [ ] Add docstrings to `IAudioRingbuffer.h`

### 7.2 Document composition pattern

- [ ] Add docstrings to `AudioPortBase.h` explaining role
- [ ] Update `AudioPort.h` docstrings explaining delegation
- [ ] Update `DummyAudioPort.h` explaining inheritance structure
- [ ] Update `InternalAudioPort.h` explaining inheritance structure

---

## Final Verification

- [ ] **Build:** `cargo build` succeeds with no warnings in refactored code
- [ ] **Unit Tests:** `target/debug/test_runner` passes
- [ ] **Integration Tests:** `./target/debug/shoopdaloop_dev.sh --self-test` passes
- [ ] Rust porting notes documented (see `port_rust.md` for MIDI, create `port_rust_audio.md` if needed)

---

## Build/Test Checkpoints Summary

| Phase | Build (`cargo build`) | Unit Tests (`test_runner`) | Integration (`--self-test`) |
|-------|----------------------|---------------------------|----------------------------|
| 1.4   |                      |                           |                            |
| 2.3   |                      |                           |                            |
| 3.4   |                      |                           |                            |
| 4.3   |                      |                           |                            |
| 5.4   |                      |                           |                            |
| 6.2   |                      |                           |                            |
| Final |                      |                           |                            |

---

## Architecture Comparison: MIDI vs Audio

| Aspect | MIDI (Refactored) | Audio (Target) |
|--------|-------------------|----------------|
| Base class | `MidiPortBase` (concrete) | `AudioPortBase` (concrete, float only) |
| State interface | `IMidiStateTracking` | `IAudioStateTracking` |
| Ringbuffer interface | `IMidiRingbuffer` | `IAudioRingbuffer` |
| Readable buffer interface | `IMidiReadableBuffer` | `IAudioReadableBuffer` |
| Writeable buffer interface | `IMidiWriteableBuffer` | `IAudioWriteableBuffer` |
| Template | No | Yes (`float` only actively used) |
| Graph wrapper | `GraphMidiPort` (composition) ✓ | `GraphAudioPort` (composition) ✓ |
| Dummy port | `DummyMidiPort` | `DummyAudioPort` |

---

## File Map

| File | Purpose |
|------|---------|
| `IAudioReadableBuffer.h` | New - readable audio buffer interface |
| `IAudioWriteableBuffer.h` | New - writeable audio buffer interface |
| `IAudioStateTracking.h` | New - audio state (peak, gain, mute) interface |
| `IAudioRingbuffer.h` | New - audio ringbuffer interface |
| `AudioPortBase.h` | New - non-template base for `float` only |
| `AudioPortBase.cpp` | New - implementation |
| `AudioPort.h` | Modified - AudioPort<float> inherits AudioPortBase |
| `AudioPort.cpp` | Modified - delegation to AudioPortBase |
| `DummyAudioPort.h/cpp` | Review - inheritance unchanged |
| `InternalAudioPort.h/cpp` | Review - inheritance unchanged |

**Note:** Only `AudioPort<float>` is actively used in codebase. `AudioPort<int>` has zero instantiations.