# AudioMidiDriver C++ dependency inventory (production path)

## Behavior parity checklist (migration gate)
- Process cycle order remains:
  1) process hook,
  2) command handling,
  3) decoupled MIDI processing,
  4) processor invocation,
  5) last_processed update.
- `wait_process` semantics unchanged (ensures at least one full process cycle boundary as currently expected by tests).
- Dummy controlled/automatic semantics unchanged:
  - controlled mode processes requested samples correctly,
  - automatic mode processes buffer-sized frames continuously,
  - pause/resume behavior unchanged.
- Telemetry parity preserved:
  - xruns,
  - sample_rate,
  - buffer_size,
  - dsp_load,
  - active,
  - last_processed,
  - client name/handle.
- C API behavior parity:
  - same success/failure behavior and error logging expectations,
  - same opaque handle lifecycle expectations (invalid/expired handle handling).

## Notes
- This checklist is used as a phase gate while swapping ownership/runtime plumbing.

This inventory captures remaining production dependencies on the C++ `AudioMidiDriver` class/inheritance.

## Core ownership and handle model
- `src/backend/libshoopdaloop_backend.cpp`
  - `g_active_drivers: std::set<shoop_shared_ptr<AudioMidiDriver>>`
  - `internal_audio_driver(shoop_audio_driver_t*)` unwraps `shoop_weak_ptr<AudioMidiDriver>`
  - `external_audio_driver(...)` wraps `shoop_shared_ptr<AudioMidiDriver>` into API handle
- This is the primary object-lifetime dependency to replace with Rust-owned driver handles.

## C API call sites depending on C++ AudioMidiDriver object
- `src/backend/libshoopdaloop_backend.cpp`
  - `create_audio_driver` via `create_audio_midi_driver(...)`
  - state getters (`get_audio_driver_state`, `maybe_driver_handle`, `maybe_driver_instance_name`, `get_sample_rate`, `get_buffer_size`, `get_driver_active`)
  - lifecycle (`start_dummy_driver`, `start_jack_driver`, `wait_process`, `destroy_audio_driver`)
  - port operations (`open_driver_audio_port`, `open_driver_midi_port`, `open_decoupled_midi_port`)
  - dummy-control endpoints (`dummy_audio_*`, `dummy_driver_*`)
- Note: dispatch has already mostly moved to type tags (`driver_type`) + static casts in backend API paths.

## C++ class hierarchy dependencies
- `src/backend/internal/AudioMidiDriver.h/.cpp`
  - abstract/virtual base class with shared functionality and Rust core field.
- `src/backend/internal/DummyAudioMidiDriver.h/.cpp`
  - derives from `AudioMidiDriver`.
- `src/backend/internal/jack/JackAudioMidiDriver.h/.cpp`
  - `GenericJackAudioMidiDriver` derives from `AudioMidiDriver`.
- `src/backend/internal/AudioMidiDrivers.cpp`
  - factory constructing `shoop_shared_ptr<AudioMidiDriver>` by backend kind.

## Rust bridge dependencies
- `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`
  - currently exposes `AudioMidiDriverCore` (state + handle registries), not full driver lifecycle/behavior.
- `src/rust/backend_rust/src/dummy_audio_midi_driver_cxx.rs`
  - partial Dummy behavior already in Rust.

## Test dependencies on C++ class internals
- Unit/integration tests in `src/backend/test/**` include and use C++ classes directly:
  - `test_DummyAudioMidiDriver.cpp`
  - `test_JackPorts.cpp`
  - `integration/test_chain_*` files using `internal_audio_driver(...)` and dummy casts
- These tests will need migration toward stable C API / Rust bridge behavior checks when class is removed.

## Priority migration targets
1. Replace API handle internals (`weak_ptr<AudioMidiDriver>`) with Rust-owned opaque driver handles.
2. Move factory creation to Rust-owned driver instances.
3. Lift driver lifecycle/process orchestration methods into Rust bridge API.
4. Declass C++ Dummy/Jack inheritance path into either Rust-native backends or narrow C callback trampolines.
5. Remove remaining direct test dependencies on C++ class internals.
