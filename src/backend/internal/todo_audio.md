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

### Key Challenges
- `AudioPort<SampleT>` is templated - need type erasure or base class pattern
- Audio buffers (`AudioBuffer<SampleT>`) use Rust buffers via CXX
- `BufferQueue<SampleT>` already wraps audio buffers

---

## Phase 1: Extract Audio Buffer Interfaces

### 1.1 Create `IAudioReadableBuffer.h`

Pure virtual interface for reading audio data.

- [ ] Create `src/backend/internal/IAudioReadableBuffer.h`
- [ ] Define methods: `audio_sample_t* get_read_ptr()`, `uint32_t n_samples()`, `void get_peak(float& in_peak, float& out_peak)`
- [ ] Document interface with docstrings

### 1.2 Create `IAudioWriteableBuffer.h`

Pure virtual interface for writing audio data.

- [ ] Create `src/backend/internal/IAudioWriteableBuffer.h`
- [ ] Define methods: `audio_sample_t* get_write_ptr()`, `uint32_t capacity()`, `void set_gain(float)`
- [ ] Document interface with docstrings

### 1.3 Update `AudioPort.h` to implement interfaces

- [ ] Make `AudioPort<SampleT>` implement `IAudioReadableBuffer` and `IAudioWriteableBuffer`
- [ ] Add `get_readable_buffer()` returning `IAudioReadableBuffer*`
- [ ] Add `get_writeable_buffer()` returning `IAudioWriteableBuffer*`
- [ ] Update `DummyAudioPort` to delegate buffer methods

### 1.4 Verify compilation and tests

- [ ] **Build:** `cargo build` succeeds
- [ ] **Tests:** `target/debug/test_runner` passes
- [ ] **Integration:** `shoopdaloop_dev.sh --self-test` passes

---

## Phase 2: Extract Audio Trait Interfaces

### 2.1 Create `IAudioStateTracking.h`

Interface for audio state (peak, gain, mute).

- [ ] Create `src/backend/internal/IAudioStateTracking.h`
- [ ] Define methods:
  - `float get_input_peak() const`
  - `void reset_input_peak()`
  - `float get_output_peak() const`
  - `void reset_output_peak()`
  - `float get_gain() const`
  - `void set_gain(float)`
  - `bool get_muted() const`
  - `void set_muted(bool)`
- [ ] Update `AudioPort<SampleT>` to implement this interface
- [ ] Document interface with docstrings

### 2.2 Create `IAudioRingbuffer.h`

Interface for audio ringbuffer operations.

- [ ] Create `src/backend/internal/IAudioRingbuffer.h`
- [ ] Define methods:
  - `void set_ringbuffer_n_samples(unsigned n)`
  - `unsigned get_ringbuffer_n_samples() const`
  - `BufferQueue<SampleT>::Snapshot get_ringbuffer_contents()`
- [ ] Update `AudioPort<SampleT>` to implement this interface
- [ ] Document interface with docstrings

### 2.3 Verify compilation and tests

- [ ] **Build:** `cargo build` succeeds
- [ ] **Tests:** `target/debug/test_runner` passes

---

## Phase 3: Create AudioPortBase

Create a non-template base class to hold core audio port logic.

### 3.1 Design Type-Erased Core

Because `AudioPort<SampleT>` is templated on `SampleT`, we need a non-templated base.

Options:
1. **Type erasure**: Use `void*` or `std::any` for sample data
2. **Separate base per type**: `AudioPortBaseFloat`, `AudioPortBaseInt`
3. **Non-template base with template internals**: Base holds void* + templates handle specifics

Recommended: Option 3 - create `AudioPortBase` that contains atomic state and delegates sample operations to template internals.

### 3.2 Create `AudioPortBase.h`

- [ ] Create `src/backend/internal/AudioPortBase.h`
- [ ] Implement non-template base:
  - `std::atomic<float> m_input_peak`
  - `std::atomic<float> m_output_peak`
  - `std::atomic<float> m_gain`
  - `std::atomic<bool> m_muted`
  - Ringbuffer pointer (non-template interface)
- [ ] Implement `IAudioStateTracking` interface
- [ ] Implement `IAudioRingbuffer` interface
- [ ] Add virtual methods for sample operations (to be overridden)
- [ ] Document composition pattern

### 3.3 Create `AudioPortBase.cpp`

- [ ] Create `src/backend/internal/AudioPortBase.cpp`
- [ ] Implement all methods from header
- [ ] Handle ringbuffer delegation

### 3.4 Verify compilation

- [ ] **Build:** `cargo build` succeeds
- [ ] **Tests:** `target/debug/test_runner` passes

---

## Phase 4: Update AudioPort with Composition

Update `AudioPort<SampleT>` to use composition.

### 4.1 Update `AudioPort.h`

- [ ] Change `AudioPort<SampleT>` to contain `AudioPortBase` internally
- [ ] Remove direct atomic members (now in AudioPortBase)
- [ ] Delegate `IAudioStateTracking` methods to AudioPortBase
- [ ] Delegate `IAudioRingbuffer` methods to AudioPortBase
- [ ] Keep template-specific methods (PROC_get_buffer, etc.)
- [ ] Maintain full backward compatibility

### 4.2 Update `AudioPort.cpp`

- [ ] Remove atomic member initializations from constructor
- [ ] Delegate all state/gain/mute methods to AudioPortBase
- [ ] Keep sample-processing logic (template-specific)
- [ ] Handle BufferQueue integration with AudioPortBase

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
| Base class | `MidiPortBase` | `AudioPortBase` |
| State interface | `IMidiStateTracking` | `IAudioStateTracking` |
| Ringbuffer interface | `IMidiRingbuffer` | `IAudioRingbuffer` |
| Readable buffer interface | `IMidiReadableBuffer` | `IAudioReadableBuffer` |
| Writeable buffer interface | `IMidiWriteableBuffer` | `IAudioWriteableBuffer` |
| Template | No | Yes (SampleT) |
| Graph wrapper | `GraphMidiPort` (composition) | `GraphAudioPort` (composition) ✓ |
| Dummy port | `DummyMidiPort` | `DummyAudioPort` |

---

## File Map

| File | Purpose |
|------|---------|
| `IAudioReadableBuffer.h` | New - readable audio buffer interface |
| `IAudioWriteableBuffer.h` | New - writeable audio buffer interface |
| `IAudioStateTracking.h` | New - audio state (peak, gain, mute) interface |
| `IAudioRingbuffer.h` | New - audio ringbuffer interface |
| `AudioPortBase.h` | New - non-template base class |
| `AudioPortBase.cpp` | New - implementation |
| `AudioPort.h` | Modified - add composition |
| `AudioPort.cpp` | Modified - add delegation |
| `DummyAudioPort.h/cpp` | Review - inheritance unchanged |
| `InternalAudioPort.h/cpp` | Review - inheritance unchanged |