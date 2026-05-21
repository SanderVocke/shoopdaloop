# Porting Plan: InternalAudioPort

## Overview

The goal is to completely port `InternalAudioPort` from C++ to Rust, eliminating the C++ wrapper class entirely. Currently, there's a hybrid architecture where the C++ `InternalAudioPort` extends `RustAudioPortF32` (which wraps Rust `AudioPort`) and delegates buffer management to a Rust `InternalAudioPort`.

## Current Architecture (Before Port)

```
C++ InternalAudioPort (wrapper)
├── inherits RustAudioPortF32
│   └── wraps Rust AudioPort (peak/gain/mute/ringbuffer)
└── delegates buffer mgmt to Rust InternalAudioPort
    └── owns: buffer, name, connectability
```

## Target Architecture (After Port)

```
Rust InternalAudioPort (fully self-contained)
├── owns: buffer, name, connectability
├── owns: AudioPort (peak/gain/mute/ringbuffer)
└── exposes full PortInterface via CXX bridge
```

## Files to Modify/Create

### Rust Side (`src/rust/backend_rust/src/`)

1. **`internal_audio_port.rs`** - Already exists, needs enhancement:
   - Include AudioPort functionality directly
   - Add methods for all PortInterface operations
   - Implement proper FFI-safe interface

2. **`internal_audio_port_cxx.rs`** - Already exists, needs extension:
   - Add CXX bindings for all new methods
   - Include gain/mute/peak methods

### C++ Side (`src/backend/internal/`)

3. **`InternalAudioPort.h`** - To be deleted (replaced by Rust)

4. **`InternalAudioPort.cpp`** - To be deleted (replaced by Rust)

5. **`RustAudioPort.h`** - Modify to support pure Rust ports:
   - Add constructor/interface for ports owned entirely in Rust
   - Potentially keep as optional wrapper or remove

### Tests

6. **`src/backend/test/unit/test_InternalAudioPort.cpp`** - May need updates to match new interface

## Phases

### Phase 1: Enhance Rust InternalAudioPort

**Goal**: Make Rust `InternalAudioPort` fully self-contained by embedding AudioPort.

1. Modify `internal_audio_port.rs`:
   ```rust
   pub struct InternalAudioPort {
       // Buffer management (existing)
       buffer: Vec<f32>,
       name: String,
       input_connectability: u32,
       output_connectability: u32,
       
       // Audio processing (NEW - from AudioPort)
       audio_port: AudioPort,
   }
   ```

2. Update methods:
   - Constructor: create AudioPort with appropriate pool settings
   - `proc_prepare()`: zero buffer
   - `proc_get_buffer()`: return buffer pointer
   - `proc_process()`: call audio_port.process_raw() with buffer
   - Add gain/mute/peak methods: delegate to audio_port

### Phase 2: Extend CXX Bridge

**Goal**: Expose all InternalAudioPort functionality to C++.

1. Update `internal_audio_port_cxx.rs`:
   ```rust
   #[cxx::bridge(namespace = "backend_rust")]
   mod ffi {
       extern "Rust" {
           type InternalAudioPort;
           
           // Existing
           fn new_internal_audio_port(...) -> Box<InternalAudioPort>;
           fn name(...) -> &str;
           fn proc_prepare(...);
           unsafe fn proc_get_buffer(...) -> *mut f32;
           fn input_connectability(...) -> u32;
           fn output_connectability(...) -> u32;
           
           // NEW - Audio processing
           fn internal_audio_port_set_gain(port: &InternalAudioPort, gain: f32);
           fn internal_audio_port_get_gain(port: &InternalAudioPort) -> f32;
           fn internal_audio_port_set_muted(port: &InternalAudioPort, muted: bool);
           fn internal_audio_port_get_muted(port: &InternalAudioPort) -> bool;
           fn internal_audio_port_get_input_peak(port: &InternalAudioPort) -> f32;
           fn internal_audio_port_get_output_peak(port: &InternalAudioPort) -> f32;
           fn internal_audio_port_reset_input_peak(port: &InternalAudioPort);
           fn internal_audio_port_reset_output_peak(port: &InternalAudioPort);
           fn internal_audio_port_set_ringbuffer_n_samples(port: &mut InternalAudioPort, n: u32);
           fn internal_audio_port_get_ringbuffer_n_samples(port: &InternalAudioPort) -> u32;
           fn internal_audio_port_get_ringbuffer_contents(port: &InternalAudioPort) -> Vec<BufferInfo>;
       }
       
       struct BufferInfo {
           data_ptr: *const f32,
           len: usize,
       }
   }
   ```

### Phase 3: Create Pure Rust Port Type for C++

**Goal**: Create a C++ wrapper that can be instantiated from C++ but is fully backed by Rust.

1. Keep C++ `InternalAudioPort` as a thin wrapper that:
   - Inherits from `PortInterface` (to maintain API compatibility)
   - Holds `rust::Box<backend_rust::InternalAudioPort>`
   - Delegates ALL methods to Rust

2. OR: Create a new `RustInternalAudioPort` that:
   - Is a `shoop_shared_ptr`-compatible type
   - Can be used in existing C++ code without inheritance

### Phase 4: Update C++ Usage Sites

**Goal**: Ensure all existing code compiles and works.

1. Files using `InternalAudioPort`:
   - `CarlaLV2ProcessingChain.cpp` - Uses `InternalAudioPort` for LV2 ports
   - `CustomProcessingChain.cpp` - Creates InternalAudioPort instances
   - `ProcessingChainInterface.h` - References `SharedInternalAudioPort`
   - Test files - Create and test InternalAudioPort

2. Update includes if needed

### Phase 5: Test and Verify

**Goal**: Ensure all functionality works correctly.

1. Run Rust unit tests: `cargo test -p backend_rust`
2. Run C++ unit tests: `./target/debug/test_runner "[InternalAudioPort]"`
3. Run integration tests
4. Verify no warnings with `RUSTFLAGS="-D warnings" cargo build`

## Build Instructions

1. **Build the project**:
   ```bash
   cd /home/sander/dev/shoopdaloop
   cargo build
   ```

2. **Run Rust tests**:
   ```bash
   cargo test -p backend_rust
   ```

3. **Run C++ tests** (after build):
   ```bash
   ./target/debug/test_runner "[InternalAudioPort]"
   ```

4. **Run all tests**:
   ```bash
   cargo test
   ```

## Key Dependencies

- `backend_rust` crate (already has AudioPort, AudioBufferQueue)
- `cxx` crate for FFI
- `crossbeam-queue` for lock-free queues
- C++ backend for integration

## Potential Challenges

1. **Buffer ownership**: The C++ code expects to manage buffer memory. Rust must provide stable pointers.

2. **Ringbuffer integration**: AudioPort already has ringbuffer; InternalAudioPort needs to coordinate with it.

3. **Thread safety**: Audio processing happens on real-time thread. All Rust code must be lock-free where possible.

4. **C++ shared_ptr compatibility**: The C++ code uses `shoop_shared_ptr<InternalAudioPort>`. Need to ensure the Rust type can be wrapped.