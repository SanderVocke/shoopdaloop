# TODO: bridge strong/weak handles for processors and decoupled MIDI ports

- [x] Orientation and baseline
  - [x] Read `porting.md`
  - [x] Review current `AudioMidiDriverRuntime`, `AudioMidiDriverCore`, `AudioMidiDriverCxxTrampolines`, and decoupled MIDI files listed in `plan.md`
  - [x] Run baseline `cargo build`

- [ ] Add bridge object infrastructure
  - [x] Add C++ bridge object module, e.g. `src/backend/internal/BridgeObject.h/.cpp`
  - [x] Define CXX-compatible `BridgeStrongHandle` and `BridgeWeakHandle`
  - [x] Define type IDs for at least processors and decoupled MIDI ports
  - [x] Implement strong handle registration for C++ `shoop_shared_ptr` objects
  - [x] Implement strong handle clone/release semantics or an equivalent RAII wrapper
  - [x] Implement downgrade from strong to weak
  - [x] Implement weak upgrade/resolve with type validation
  - [x] Add typed C++ helper/wrapper functions for `HasAudioProcessingFunction`
  - [x] Add typed C++ helper/wrapper functions for `DecoupledMidiPort`
  - [x] Add Rust/CXX bridge definitions for handle structs and required operations, or expose generated structs in a way both languages can use
  - [ ] Document real-time/process-thread limitations of the first registry implementation
  - [x] Milestone build: `cargo build`
  - nuance: first registry implementation currently uses a global mutex-protected C++ registry and typed holder erasure; no process-thread policy docs added yet.

- [x] Milestone 1: migrate processors to bridge weak handles
  - [x] Update Rust `AudioMidiDriverCore` processor storage from raw `usize` pointers to bridge weak handles
  - [x] Update Rust `add_processor`/`remove_processor` APIs and CXX bridge declarations
  - [x] Update Rust process-cycle dispatch to call a processor bridge trampoline instead of raw pointer trampoline
  - [x] Update C++ `AudioMidiDriverRuntime::add_processor` to register/downgrade processor handles through the bridge system
  - [x] Update C++ `AudioMidiDriverRuntime::remove_processor` to unregister Rust handles and release bridge registration state
  - [x] Replace or retire `audiomididriver_process_processor(uintptr_t, ...)` raw pointer dispatch
  - [x] Add C++ trampoline that upgrades/resolves processor bridge weak handles and calls `PROC_process` only when valid
  - [x] Preserve public `processors()` compatibility behavior
  - nuance: due CXX type limitations, bridge weak handles are passed across Rust/C++ as `(id,type_id)` scalar arguments rather than as a shared bridge-struct type in `audio_midi_driver_cxx.rs` extern signatures.

- [x] Milestone 1 tests and validation
  - [x] Add/adjust tests for normal processor registration and processing
  - [x] Add/adjust tests for processor removal preventing further dispatch
  - [x] Add/adjust tests for stale processor weak handles being safe no-ops
  - [x] Add/adjust tests for repeated processor add/remove cycles under dummy controlled processing
  - [x] Run `cargo build`
  - [x] Run `cargo test`
  - [x] Run backend `test_runner`
  - [x] Fix all warnings/errors introduced by processor migration
  - nuance: added `DummyAudioMidiDriver - processor add/remove cycles` unit test covering processing after add, no further processing after remove, and repeated add/remove stability.

- [ ] Milestone 2: migrate decoupled MIDI ports to bridge handles
  - [ ] Update Rust `AudioMidiDriverCore` decoupled port storage from raw `usize` pointers to bridge handles
  - [ ] Update Rust decoupled registration/unregistration APIs and CXX bridge declarations
  - [ ] Update Rust `process_decoupled_port`, `close_decoupled_port`, and process-cycle dispatch to use bridge handles
  - [ ] Update C++ `AudioMidiDriverRuntime::make_decoupled_midi_port` to register decoupled ports through the bridge system
  - [ ] Replace C++ direct `shared_ptr` decoupled keepalive map with bridge strong handle registration records
  - [ ] Update C++ `AudioMidiDriverRuntime::unregister_decoupled_midi_port` to release bridge strong handles on unregister
  - [ ] Replace or retire raw pointer decoupled trampolines
  - [ ] Add C++ trampolines that upgrade/resolve decoupled bridge handles and call `PROC_process`/`close` only when valid
  - [ ] Preserve C API and C++ API behavior for decoupled MIDI ports

- [ ] Milestone 2 tests and validation
  - [ ] Add/adjust tests for normal decoupled MIDI send/receive behavior
  - [ ] Add/adjust tests for close/unregister during active dummy processing
  - [ ] Add/adjust tests for repeated decoupled open/close cycles
  - [ ] Add/adjust tests for stale decoupled handle operations being safe
  - [ ] Add/adjust tests for registered decoupled port keepalive behavior if intended by the selected strong handle policy
  - [ ] Run `cargo build`
  - [ ] Run `cargo test`
  - [ ] Run backend `test_runner`
  - [ ] Run `./target/debug/shoopdaloop_dev.sh --self-test`
  - [ ] Fix all warnings/errors introduced by decoupled migration

- [ ] Final cleanup and verification
  - [ ] Confirm Rust `AudioMidiDriverCore` no longer stores processor raw object pointers as `usize`
  - [ ] Confirm Rust `AudioMidiDriverCore` no longer stores decoupled MIDI raw object pointers as `usize`
  - [ ] Confirm C++ decoupled keepalive is represented by bridge strong handles, not ad hoc direct `shared_ptr` maps
  - [ ] Confirm stale bridge weak handles are safe no-ops
  - [ ] Remove obsolete raw pointer trampoline declarations/definitions if no longer used
  - [ ] Clean up includes and comments in affected files
  - [ ] Run `cargo fmt --all`
  - [ ] Run `RUSTFLAGS="-D warnings" cargo build`
  - [ ] Run `cargo test`
  - [ ] Run backend `test_runner`
  - [ ] Run `./target/debug/shoopdaloop_dev.sh --self-test`
  - [ ] Confirm final state: all tests pass, strict build passes, Rust formatting applied
